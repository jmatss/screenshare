use std::{collections::HashMap, error::Error, fs, io, process::Stdio, sync::Arc, time::Duration};

use async_tungstenite::{
    stream::Stream,
    tokio::{connect_async, ConnectStream, TokioAdapter},
    tungstenite, WebSocketStream,
};
use clap::Arg;
use event_listener::Event;
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use h264::H264Reader;
use tokio::{
    net::TcpStream,
    process::{ChildStdout, Command},
    sync::Mutex,
};
use tokio_native_tls::TlsStream;
use webrtc::{
    api::{
        interceptor_registry::register_default_interceptors,
        media_engine::{MediaEngine, MIME_TYPE_H264},
        APIBuilder,
    },
    ice_transport::ice_server::RTCIceServer,
    interceptor::registry::Registry,
    media::Sample,
    peer_connection::{
        configuration::RTCConfiguration, sdp::session_description::RTCSessionDescription,
        RTCPeerConnection,
    },
    rtp_transceiver::rtp_codec::RTCRtpCodecCapability,
    track::track_local::{track_local_static_sample::TrackLocalStaticSample, TrackLocal},
};

use crate::message::{Message, RTCIceCandidateInitFix};

mod h264;
mod message;

type WebSocketTx = SplitSink<
    WebSocketStream<Stream<TokioAdapter<TcpStream>, TokioAdapter<TlsStream<TcpStream>>>>,
    tungstenite::Message,
>;
type WebSocketRx = SplitStream<
    WebSocketStream<Stream<TokioAdapter<TcpStream>, TokioAdapter<TlsStream<TcpStream>>>>,
>;

pub const DEFAULT_STUN_SERVER: &str = "stun:stun.l.google.com:19302";
pub const DEFAULT_CONFIG_PATH: &str = "ffmpeg.cfg";

pub const OFFER: &str = "offer";
pub const ANSWER: &str = "answer";
pub const CANDIDATE: &str = "candidate";
/// Sent from the signaling server to the producer when a new client is connected.
pub const CONNECT: &str = "connect";
/// Disonnect message sent from signaling server to producer when a client
/// websocket connection is torn down.
pub const DISCONNECT: &str = "disconnect";

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn Error>> {
    let matches = clap::Command::new("skitspel")
        .arg(
            Arg::new("url")
                .short('u')
                .long("url")
                .value_name("URL")
                .help("The URL of the websocket server which we will connect to.")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::new("stun")
                .short('s')
                .long("stun")
                .help("The URL of the stun server which we should use when setting up the RTC connections. \
                If not set, a default google stun server will be used.")
                .default_value(DEFAULT_STUN_SERVER)
                .takes_value(true)
                .required(false),
        )
        .arg(
            Arg::new("cfg")
                .short('c')
                .long("config")
                .help("Path to the config file containing the arguments to the FFMPEG command.")
                .default_value(DEFAULT_CONFIG_PATH)
                .takes_value(true)
                .required(false),
        )
        .get_matches();

    let ws_url = matches.value_of("url").unwrap();
    let stun_url = matches.value_of("stun").unwrap().to_string();
    let cfg_path = matches.value_of("cfg").unwrap().to_string();

    let (ws_stream, _) = connect_async(ws_url).await?;
    let h264_reader = video_stream_reader(&cfg_path)?;

    let exit_event_rx = Arc::new(Event::new());
    let video_track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_H264.into(),
            ..Default::default()
        },
        "video".into(),
        "screenshare".into(),
    ));

    let exit_event_tx = Arc::clone(&exit_event_rx);
    let video_track_tx = Arc::clone(&video_track);
    tokio::spawn(async move {
        if let Err(err) = video_stream_writer(video_track_tx, h264_reader).await {
            println!("{}", err);
        }
        exit_event_tx.notify(usize::MAX);
    });

    let exit_event_tx = Arc::clone(&exit_event_rx);
    let video_track_tx = Arc::clone(&video_track);
    tokio::spawn(async move {
        if let Err(err) = websocket_listener(ws_stream, video_track_tx, &stun_url).await {
            println!("{}", err);
        }
        exit_event_tx.notify(usize::MAX);
    });

    exit_event_rx.listen().wait();
    println!("DONE!");

    Ok(())
}

/// Creates a `H264Reader` that reads an asynchronous video stream from stdout.
/// The stream is provided by `ffmpeg` and the options used when launcing the
/// ffmpeg process are specified in the file with path `cfg_path`.
fn video_stream_reader(cfg_path: &str) -> io::Result<H264Reader<ChildStdout>> {
    let cmd = Command::new("ffmpeg")
        .args(&fs::read_to_string(cfg_path)?.split(' ').collect::<Vec<_>>())
        .stdout(Stdio::piped())
        .spawn()?;
    Ok(H264Reader::new(cmd.stdout.unwrap()))
}

/// An infinite loop that reads the next NAL from the `h262_reader` and writes it
/// to the given `video_track`.
async fn video_stream_writer(
    video_track: Arc<TrackLocalStaticSample>,
    mut h264_reader: H264Reader<ChildStdout>,
) -> webrtc::error::Result<()> {
    println!("Starting to write video to track.");

    loop {
        let nal = h264_reader.next_nal().await?;
        //println!("{}", nal.unit_type);
        video_track
            .write_sample(&Sample {
                data: nal.data.freeze(),
                duration: Duration::from_secs(1),
                ..Default::default()
            })
            .await?;
    }
}

/// Listens for messages on the given `ws_stream` and handles them accordingly.
async fn websocket_listener(
    ws_stream: WebSocketStream<ConnectStream>,
    video_track: Arc<TrackLocalStaticSample>,
    stun_url: &str,
) -> io::Result<()> {
    println!("Started websocket listener.");

    let (ws_tx, mut ws_rx) = ws_stream.split();
    let ws_tx = Arc::new(Mutex::new(ws_tx));
    let mut rtc_conns = HashMap::default();

    loop {
        if let Err(err) = handle_message(
            &mut rtc_conns,
            Arc::clone(&ws_tx),
            &mut ws_rx,
            Arc::clone(&video_track),
            stun_url,
        )
        .await
        {
            println!("Error in websocket_handler: {}", err);
        }
    }
}

/// Teceive the next message on the websocket connection and handles it accordingly.
async fn handle_message(
    rtc_conns: &mut HashMap<String, RTCPeerConnection>,
    ws_tx: Arc<Mutex<WebSocketTx>>,
    ws_rx: &mut WebSocketRx,
    video_track: Arc<TrackLocalStaticSample>,
    stun_url: &str,
) -> io::Result<()> {
    let tungstenite_msg_result = match ws_rx.next().await {
        Some(msg_result) => msg_result,
        None => {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Websocket closed when reading message.".to_string(),
            ));
        }
    };

    let tungstenite_msg = match tungstenite_msg_result {
        Ok(tungstenite_msg) => tungstenite_msg,
        Err(err) => {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Received error on websocket: {}", err),
            ));
        }
    };

    let tungstenite_msg_bin = tungstenite_msg.into_data();
    if tungstenite_msg_bin.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Received empty packet on websocket.".to_string(),
        ));
    }

    let msg = match std::str::from_utf8(&tungstenite_msg_bin) {
        Ok(raw_msg) => match serde_json::from_str::<Message>(raw_msg) {
            Ok(msg) => msg,
            Err(err) => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!(
                        "Unable to parse message as json.\nMessage: {:?}\nErr: {}",
                        raw_msg, err
                    ),
                ));
            }
        },
        Err(err) => {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "Error when parsing message as UTF-8.\nMessage: {:?}\nErr: {}",
                    tungstenite_msg_bin, err
                ),
            ));
        }
    };

    match msg.action.as_str() {
        CONNECT => {
            handle_connect(
                rtc_conns,
                Arc::clone(&ws_tx),
                Arc::clone(&video_track),
                stun_url,
                &msg,
            )
            .await
        }

        DISCONNECT => handle_disconnect(rtc_conns, &msg).await,

        OFFER => handle_offer(rtc_conns, Arc::clone(&ws_tx), &msg).await,

        CANDIDATE => handle_candidate(rtc_conns, &msg).await,

        _ => Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "Received invalid action in message from client with ID {}: {}",
                msg.id, msg.action
            ),
        )),
    }
}

/// Handles a received "connect" message.
async fn handle_connect(
    rtc_conns: &mut HashMap<String, RTCPeerConnection>,
    ws_tx: Arc<Mutex<WebSocketTx>>,
    video_track: Arc<TrackLocalStaticSample>,
    stun_url: &str,
    msg: &Message,
) -> io::Result<()> {
    println!("WebRTC connect from client with ID: {}", msg.id);

    let rtc_conn = setup_rtc_connection(
        Arc::clone(&ws_tx),
        Arc::clone(&video_track),
        stun_url,
        msg.id.clone(),
    )
    .await
    .map_err(to_io_err)?;
    rtc_conns.insert(msg.id.clone(), rtc_conn);

    Ok(())
}

/// Handles a received "disconnect" message.
async fn handle_disconnect(
    rtc_conns: &mut HashMap<String, RTCPeerConnection>,
    msg: &Message,
) -> io::Result<()> {
    println!("WebRTC disconnect from client with ID: {}", msg.id);

    rtc_conns.remove(&msg.id);

    Ok(())
}

/// Handles a received "offer" message.
async fn handle_offer(
    rtc_conns: &HashMap<String, RTCPeerConnection>,
    ws_tx: Arc<Mutex<WebSocketTx>>,
    msg: &Message,
) -> io::Result<()> {
    println!("WebRTC offer from client with ID: {}", msg.id);

    let remote_desc = serde_json::from_str::<RTCSessionDescription>(&msg.data).map_err(|err| {
        io::Error::new(
            io::ErrorKind::Other,
            format!(
                "Unable to parse offer from client with ID {}: {}",
                msg.id, err
            ),
        )
    })?;

    if let Some(rtc_conn) = rtc_conns.get(&msg.id) {
        rtc_conn
            .set_remote_description(remote_desc)
            .await
            .map_err(to_io_err)?;

        let local_desc = rtc_conn.create_answer(None).await.map_err(to_io_err)?;
        let local_desc_str = serde_json::to_string(&local_desc).map_err(|err| {
            io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "Unable to create local desc for client with ID {}: {}",
                    msg.id, err
                ),
            )
        })?;

        let response_msg = Message {
            id: msg.id.clone(),
            action: ANSWER.to_string(),
            data: local_desc_str,
        };
        send(Arc::clone(&ws_tx), response_msg)
            .await
            .map_err(to_io_err)?;

        rtc_conn
            .set_local_description(local_desc)
            .await
            .map_err(to_io_err)?;

        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "Unable to find connection with ID {} when receiving offer.",
                msg.id,
            ),
        ))
    }
}

/// Handles a received "candidate"/"candidate_client" message.
async fn handle_candidate(
    rtc_conns: &HashMap<String, RTCPeerConnection>,
    msg: &Message,
) -> io::Result<()> {
    println!("WebRTC ICE candidate from ID: {}", msg.id);

    let candidate = serde_json::from_str::<RTCIceCandidateInitFix>(&msg.data)
        .map_err(|err| {
            io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "Unable to parse ICE candidate from client with ID {}: {}",
                    msg.id, err
                ),
            )
        })
        .unwrap();

    if let Some(rtc_conn) = rtc_conns.get(&msg.id) {
        rtc_conn
            .add_ice_candidate(candidate.fix_to())
            .await
            .map_err(to_io_err)?;

        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "Unable to find connection with ID {} when receiving candidate.",
                msg.id,
            ),
        ))
    }
}

/// Sets up a new RTC connection.
async fn setup_rtc_connection(
    ws_tx: Arc<Mutex<WebSocketTx>>,
    video_track: Arc<TrackLocalStaticSample>,
    stun_url: &str,
    client_id: String,
) -> webrtc::error::Result<RTCPeerConnection> {
    // TODO: Only register h264 codecs? Is probably the only thing that we will
    //       be using, so unnecessary to register every default codec.
    let mut media_engine = MediaEngine::default();
    media_engine.register_default_codecs()?;

    let mut registry = Registry::new();
    registry = register_default_interceptors(registry, &mut media_engine)?;

    let api = APIBuilder::new()
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .build();

    let config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: vec![stun_url.into()],
            ..Default::default()
        }],
        ..Default::default()
    };

    let rtc_conn = api.new_peer_connection(config).await?;

    // TODO: Do we need to listen for traffic on the `rtp_sender`? What kind
    //       of traffic can we receive on it?
    rtc_conn
        .add_track(Arc::clone(&video_track) as Arc<dyn TrackLocal + Send + Sync>)
        .await?;

    let client_id_ice = client_id.clone();
    rtc_conn
        .on_ice_connection_state_change(Box::new(move |ice_conn_state| {
            println!(
                "ICE Connection State has changed for client with ID {}: {}",
                client_id_ice, ice_conn_state
            );
            Box::pin(async {})
        }))
        .await;

    let client_id_peer = client_id.clone();
    rtc_conn
        .on_peer_connection_state_change(Box::new(move |peer_conn_state| {
            println!(
                "Peer Connection State has changed for client with ID {}: {}",
                client_id_peer, peer_conn_state
            );
            Box::pin(async {})
        }))
        .await;

    rtc_conn
        .on_ice_candidate(Box::new(move |ice_candidate| {
            let ws_tx = Arc::clone(&ws_tx);
            let client_id = client_id.clone();
            Box::pin(async move {
                if let Some(ice_candidate) = ice_candidate {
                    let candidate = ice_candidate
                        .to_json()
                        .await
                        .map_err(|err| {
                            format!(
                                "Unable to convert ICE candidate to json for client with ID {}: {}",
                                client_id, err
                            )
                        })
                        .unwrap();

                    let candidate_json = serde_json::to_string(&candidate)
                        .map_err(|err| {
                            format!(
                                "Unable to parse ICE candidate from json for client with ID {}: {}",
                                client_id, err
                            )
                        })
                        .unwrap();

                    let msg = Message {
                        id: client_id.clone(),
                        action: CANDIDATE.to_string(),
                        data: candidate_json,
                    };
                    send(Arc::clone(&ws_tx), msg)
                        .await
                        .map_err(|err| {
                            format!(
                                "Unable to send ice candidate to client with ID {}: {}",
                                client_id, err
                            )
                        })
                        .unwrap();
                }
            })
        }))
        .await;

    Ok(rtc_conn)
}

/// Send the given `msg` over the given `ws_tx` websocket stream.
async fn send(ws_tx: Arc<Mutex<WebSocketTx>>, msg: Message) -> webrtc::error::Result<()> {
    let mut ws_tx_guard = ws_tx.lock().await;
    ws_tx_guard
        .send(tungstenite::Message::Text(
            serde_json::to_string(&msg).map_err(|e| webrtc::error::Error::new(format!("{}", e)))?,
        ))
        .await
        .map_err(|e| webrtc::error::Error::new(format!("{}", e)))
}

fn to_io_err(err: webrtc::Error) -> io::Error {
    io::Error::new(io::ErrorKind::Other, format!("WebRTC error -- {}", err))
}
