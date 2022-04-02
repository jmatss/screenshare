use std::{
    collections::HashMap,
    error::Error,
    fs, io,
    process::Stdio,
    sync::Arc,
    time::{Duration, Instant},
};

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
    ice_transport::{ice_candidate::RTCIceCandidateInit, ice_server::RTCIceServer},
    interceptor::registry::Registry,
    media::Sample,
    peer_connection::{
        configuration::RTCConfiguration, peer_connection_state::RTCPeerConnectionState,
        sdp::session_description::RTCSessionDescription, RTCPeerConnection,
    },
    rtp_transceiver::rtp_codec::RTCRtpCodecCapability,
    track::track_local::{track_local_static_sample::TrackLocalStaticSample, TrackLocal},
};

use crate::message::{Message, RTCIceCandidateInitFix};

mod h264;
mod message;

type TokioWebSocketStream =
    WebSocketStream<Stream<TokioAdapter<TcpStream>, TokioAdapter<TlsStream<TcpStream>>>>;
type WebSocketTx = SplitSink<TokioWebSocketStream, tungstenite::Message>;
type WebSocketRx = SplitStream<TokioWebSocketStream>;

/// The `ConnectionId` is used as the unique identifier in AWS API Gateway
/// websocket implementation. Every new websocket connection is assigned a
/// unique ConnectionId when the connection is established. We use this to specify
/// which websocket connection to send responses to.
type ConnectionId = String;

/// The clients might start sending ICE candidates before we have received their
/// remote descriptions. So we will need to buffer the candidates temporarily in
/// this structure and only register them once the remote description have been
/// registered for the given websocket connect.
type CandidatesBuffer = Arc<Mutex<HashMap<ConnectionId, Vec<RTCIceCandidateInit>>>>;

pub const DEFAULT_STUN_SERVER: &str = "stun:stun.l.google.com:19302";
pub const DEFAULT_CONFIG_PATH: &str = "ffmpeg.cfg";

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
        println!("{}", nal.unit_type);
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
    let candidates_buffer = Arc::new(Mutex::new(HashMap::default()));

    let ws_tx_clone = Arc::clone(&ws_tx);
    tokio::spawn(async move { websocket_pinger(ws_tx_clone).await });

    loop {
        if let Err(err) = handle_message(
            &mut rtc_conns,
            Arc::clone(&candidates_buffer),
            Arc::clone(&ws_tx),
            &mut ws_rx,
            Arc::clone(&video_track),
            stun_url,
        )
        .await
        {
            if let io::ErrorKind::BrokenPipe = err.kind() {
                return Err(err); // Websocket disconnected.
            } else {
                println!("Error in websocket_handler: {}", err);
            }
        }
    }
}

/// Sends a websocket `ping` message over the `ws_tx` websocket stream periodically
/// every 9 minutes. The default keep-alive time for the API Gateway websockets
/// are 10 minutes, so make sure to send at least one keep-alive message in that
/// time.
async fn websocket_pinger(ws_tx: Arc<Mutex<WebSocketTx>>) -> io::Result<()> {
    let start = Instant::now();
    loop {
        tokio::time::sleep(Duration::from_secs(60 * 9)).await;
        println!(
            "Sending a ping to signaling server (elapsed: {:?})",
            start.elapsed()
        );
        send_ping(Arc::clone(&ws_tx)).await.map_err(to_io_err)?;
    }
}

/// Teceive the next message on the websocket connection and handles it accordingly.
async fn handle_message(
    rtc_conns: &mut HashMap<ConnectionId, RTCPeerConnection>,
    candidates_buffer: CandidatesBuffer,
    ws_tx: Arc<Mutex<WebSocketTx>>,
    ws_rx: &mut WebSocketRx,
    video_track: Arc<TrackLocalStaticSample>,
    stun_url: &str,
) -> io::Result<()> {
    let tungstenite_msg_result = match ws_rx.next().await {
        Some(msg_result) => msg_result,
        None => {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
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

    // We only expect text messages or ping/pong messages. The ping/pong messages
    // are used to keep the websocket connection alive.
    let raw_msg = match tungstenite_msg {
        tungstenite::Message::Text(text) => text,
        tungstenite::Message::Ping(_) => return handle_ping(ws_tx).await,
        tungstenite::Message::Pong(_) => return Ok(()),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "Received unexpected packet on websocket: {:#?}",
                    tungstenite_msg
                ),
            ))
        }
    };

    let msg = match serde_json::from_str::<Message>(&raw_msg) {
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
    };

    match msg {
        Message::Candidate {
            id,
            data: candidate,
        } => {
            handle_candidate(rtc_conns, candidates_buffer, id, candidate.fix_to()).await?;
        }

        Message::Disconnect { id } => {
            rtc_conns.remove(&id);
        }

        Message::Offer {
            id,
            data: remote_desc,
        } => {
            let rtc_conn = handle_offer(
                Arc::clone(&ws_tx),
                Arc::clone(&video_track),
                Arc::clone(&candidates_buffer),
                stun_url,
                id.clone(),
                remote_desc,
            )
            .await?;
            rtc_conns.insert(id, rtc_conn);
        }

        Message::Response { r#type, data } => {
            println!(
                "Received reponse from signaling server. type: {}, message: {}",
                r#type, data
            );
        }

        Message::Answer {
            id,
            data: remote_desc,
        } => {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "Received unexpected answer ffrom client with ID {}. Remote desc: {:#?}",
                    id, remote_desc
                ),
            ))
        }
    }

    Ok(())
}

/// Handles a received Websocket "ping" message. Need to respond with a "pong".
async fn handle_ping(ws_tx: Arc<Mutex<WebSocketTx>>) -> io::Result<()> {
    println!("Received WebRTC ping from signaling server.");
    send_pong(ws_tx).await.map_err(to_io_err)
}

/// Handles a received "offer" message.
async fn handle_offer(
    ws_tx: Arc<Mutex<WebSocketTx>>,
    video_track: Arc<TrackLocalStaticSample>,
    candidates_buffer: CandidatesBuffer,
    stun_url: &str,
    client_id: ConnectionId,
    remote_desc: RTCSessionDescription,
) -> io::Result<RTCPeerConnection> {
    println!("Received WebRTC offer from client with ID: {}", client_id);

    let rtc_conn = setup_rtc_connection(
        Arc::clone(&ws_tx),
        Arc::clone(&video_track),
        stun_url,
        client_id.clone(),
    )
    .await
    .map_err(to_io_err)?;

    rtc_conn
        .set_remote_description(remote_desc)
        .await
        .map_err(to_io_err)?;

    let local_desc = rtc_conn.create_answer(None).await.map_err(to_io_err)?;
    let response_msg = Message::Answer {
        id: client_id.clone(),
        data: local_desc.clone(),
    };

    send(Arc::clone(&ws_tx), response_msg)
        .await
        .map_err(to_io_err)?;

    rtc_conn
        .set_local_description(local_desc)
        .await
        .map_err(to_io_err)?;

    // Now that the remote description have been set, we can register any
    // buffered candidates from the remote client.
    let mut candidates_buffer_guard = candidates_buffer.lock().await;
    if let Some(candidates) = candidates_buffer_guard.remove(&client_id) {
        println!(
            "Registering {} candidates from client with ID {}",
            candidates.len(),
            client_id
        );
        for candidate in candidates {
            rtc_conn
                .add_ice_candidate(candidate)
                .await
                .map_err(to_io_err)?;
        }
    }

    Ok(rtc_conn)
}

/// Handles a received "candidate"/"candidate_client" message.
async fn handle_candidate(
    rtc_conns: &HashMap<ConnectionId, RTCPeerConnection>,
    candidates_buffer: CandidatesBuffer,
    id: ConnectionId,
    candidate: RTCIceCandidateInit,
) -> io::Result<()> {
    println!("Received WebRTC ICE candidate from client with ID: {}", id);

    // Need to lock buffers before checking the state of the connection to
    // handle edge case where the connection state changes to `Connected` just
    // before we insert the candidate in the buffer. In those cases we now
    // force the "task"/"thread" that is in the process of moving the candidates
    // from the buffers to wait until we have inserted this candidate.
    let mut candidates_buffer_guard = candidates_buffer.lock().await;

    if let Some(rtc_conn) = rtc_conns.get(&id) {
        if let RTCPeerConnectionState::Connected = rtc_conn.connection_state() {
            rtc_conn
                .add_ice_candidate(candidate)
                .await
                .map_err(to_io_err)?;
        } else {
            candidates_buffer_guard
                .entry(id)
                .or_default()
                .push(candidate);
        }
    } else {
        candidates_buffer_guard
            .entry(id)
            .or_default()
            .push(candidate);
    }

    Ok(())
}

/// Sets up a new RTC connection and the callback functions to handle the
/// communication over the RTC connection.
async fn setup_rtc_connection(
    ws_tx: Arc<Mutex<WebSocketTx>>,
    video_track: Arc<TrackLocalStaticSample>,
    stun_url: &str,
    client_id: ConnectionId,
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
        .add_track(Arc::clone(&video_track) as Arc<dyn TrackLocal + Send + Sync>)
        .await?;

    let client_id_candidate = client_id.clone();
    rtc_conn
        .on_ice_candidate(Box::new(move |ice_candidate| {
            let ws_tx = Arc::clone(&ws_tx);
            let client_id = client_id_candidate.clone();
            Box::pin(async move {
                if let Some(candidate) = ice_candidate {
                    let candidate_init = candidate
                        .to_json()
                        .await
                        .map_err(|err| {
                            format!(
                                "Unable to convert ICE candidate to json for client with ID {}: {}",
                                client_id, err
                            )
                        })
                        .unwrap();

                    let msg = Message::Candidate {
                        id: client_id.clone(),
                        data: RTCIceCandidateInitFix::fix_from(candidate_init),
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

/// Send a websocket `ping` over the given `ws_tx` websocket stream.
async fn send_ping(ws_tx: Arc<Mutex<WebSocketTx>>) -> webrtc::error::Result<()> {
    let mut ws_tx_guard = ws_tx.lock().await;
    ws_tx_guard
        .send(tungstenite::Message::Ping(Vec::with_capacity(0)))
        .await
        .map_err(|e| webrtc::error::Error::new(format!("{}", e)))
}

/// Send a websocket `pong` over the given `ws_tx` websocket stream.
async fn send_pong(ws_tx: Arc<Mutex<WebSocketTx>>) -> webrtc::error::Result<()> {
    let mut ws_tx_guard = ws_tx.lock().await;
    ws_tx_guard
        .send(tungstenite::Message::Pong(Vec::with_capacity(0)))
        .await
        .map_err(|e| webrtc::error::Error::new(format!("{}", e)))
}

fn to_io_err(err: webrtc::Error) -> io::Error {
    io::Error::new(io::ErrorKind::Other, format!("WebRTC error -- {}", err))
}
