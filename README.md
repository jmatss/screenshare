# Screenshare
Screensharing via WebRTC from a single `producer` to web client `consumer`'s. The initial communication between the `producer` and a `consumer` is done via the `signaling` component over websockets. Once the `producer` and `consumer` have decided how to send the video stream between each other, they will establish a direct connection and send the video stream over that connection.


## Message format
The WebRTC signaling message are sent as json. Format of messages:
```
{
    "action" : "<ACTION>",
    "data": "<DATA>"
}
```

The `data` field can hold arbitrary data and its format differs depending on the type of message that is being set.
The `action` field represents the type of the message that is being sent. List of valid `action` values:
- answer
- offer
- candidate
- response

The three first items in the list represents messages that are sent from hosts to signaling server. The last value is used by the signaling server when responding to messages from a host.


Messages sent from the signaling server to the hosts might have some extra optional fields depending on the message that is being sent:
- The `producer` communicates over a single websocket connection to the signaling server, so the signaling server needs to add extra information in the messages so that the `producer` knows which `consumer` sent the message. This is handled in the signaling server by adding a extra `id` field in the messages sent to the `producer`. This `id` field contains a unique identifier for a specific client.
- The signaling server sets the `action` field to `response` when responding to the hosts, a extra `type` field is added that stores the original `action` value set by the host. 


# Components
The application is divided into three modular components. The components communicate with each other over websockets and WebRTC.

## Signaling
The `signaling` component handles the WebRTC signaling between the `producer` and `consumer`'s. The signaling logic is designed to be hosted in AWS. It consist of multiple Java modules where every module represents a Lambda function.
The name of the child module represents the `route key` that should be used in the API gateway that calls the Lambda function.

When setting up a websocket connection via AWS API Gateway, a `consumer` and `producer` are differentiated by the `type` parameter in the URL.
So when connecting as a `producer` to the signaling server, the `type` parameter should be set to `producer` in the URL like so:
```
wss://<API_ID>.execute-api.<REGION>.amazonaws.com/<STAGE>?type=producer
```
A `consumer` doesn't need to specify any parameter since a empty `type` parameter is assumed to represent a `consumer`.

### Requirements
- Java (Version known to work: 8)
- Maven (Version known to work: 3.6.1)

### Build
The following command should be ran in the parent/aggregate folder:
```
mvn clean install
```

### Install
The jars generated during build should be uploaded as the code for AWS Lambda functions.


## Producer
The `producer` produces the video stream and shares its screen. The implementation is written in `rust` and uses `ffmpeg` to record the screen.

### Build
```
cargo build --release
```

### Run
```
USAGE:
    screenshare.exe [OPTIONS] --url <URL>

OPTIONS:
    -c, --config <cfg>    Path to the config file containing the arguments to the FFMPEG command.
                          [default: ffmpeg.cfg]
    -h, --help            Print help information
    -s, --stun <stun>     The URL of the stun server which we should use when setting up the RTC
                          connections. If not set, a default google stun server will be used.
                          [default: stun:stun.l.google.com:19302]
    -u, --url <URL>       The URL of the websocket server which we will connect to.
```

### Requirements
- Rust (Version know to work: 1.57.0)
- Cargo (Version know to work: 1.57.0)
- ffmpeg (Version known to work: 4.4.1)

For the default settings stored in `ffmpeg.cfg`, ffmpeg must have support for `x264` and `gdigrab` (windows only). But these settings can be changed at runtime, so the only real requirement is a working `ffmpeg` binary.


## Consumer
A webclient written in Javascript. The `consumer` is expected to send the WebRTC `offer` while the `producer` sends `answer`'s.

### Run
Specify the domain name of the signaling server and click connect. The video stream from the `producer` should soon start playing in the HTML `video` element of the client.
