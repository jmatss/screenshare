var webSocket;

var rtcConn;

/// URL of STUN server to use when setting up WebRTC connection.
const STUN_URL = "stun:stun.l.google.com:19302";

/// Values set in the `action` field indicating what type of message that is
/// being sent over the websocket connection.
const OFFER = "offer";
const ANSWER = "answer";
const CANDIDATE = "candidate";

async function setupRtc() {
    rtcConn = new RTCPeerConnection({
        iceServers: [{
            urls: STUN_URL
        }]
    });

    // Ran when a new track is added to the `rtcConn` on the remote side.
    // This track will always be video stream and it should be assigned to the
    // video element which will display the track.
    rtcConn.ontrack = event => {
        vidElem = document.getElementById("video");
        vidElem.srcObject = event.streams[0];
    };

    rtcConn.onicecandidate = event => {
        if (event.candidate) {
            const candidate = JSON.stringify(event.candidate.toJSON());
            webSocket.send(message(CANDIDATE, candidate));
        }
    };

    rtcConn.oniceconnectionstatechange = _ => {
        console.log("ICE Connection State changed: " + rtcConn.iceConnectionState);
    };
}

function connect() {
    if (!("WebSocket" in window)) {
        return alert("Websockets not supported in this browser.");
    } else if (typeof webSocket !== "undefined") {
        return alert("Already connected.");
    }

    let localWebSocket;
    try {
        const url = document.getElementById("url").value;
        console.log("Connecting to URL: " + url);
        localWebSocket = new WebSocket(url);
    } catch (e) {
        const msg = "Unable to connect to URL: " + url.value + "\n"
                  + "Make sure that the URL is formatted correctly (must include protocol).";
        return alert(msg);
    }

    localWebSocket.onopen = function (_) {
        webSocket = localWebSocket;
        sendRtcOffer();
    };

    localWebSocket.onclose = function (_) {
        if (typeof webSocket !== "undefined") {
            alert("Connection to server closed.");
            webSocket = undefined;
        } else {
            alert("Unable to connect to server.");
        }
    };

    localWebSocket.onmessage = onmessageHandler;
}

function disconnect() {
    if (typeof webSocket !== "undefined") {
        webSocket.close();
    }
}

async function sendRtcOffer() {
    rtcConn.addTransceiver('video', { 'direction': 'recvonly' });
    const localDesc = await rtcConn.createOffer();
    await rtcConn.setLocalDescription(localDesc);
    const offer = JSON.stringify(rtcConn.localDescription.toJSON());
    console.log("Sending RTC offer: " + offer);
    webSocket.send(message(OFFER, offer));
}

/// Reads & handles the data received from the server over the websocket.
async function onmessageHandler(event) {
    console.log("Received data on websocket: " + event.data);
    const msg = JSON.parse(event.data);

    switch (msg.action) {
        case OFFER:
            console.log("ERROR - Should never get OFFER from server.");
            break;

        case ANSWER:
            const answer = JSON.parse(msg.data);
            const remoteDesc = new RTCSessionDescription(answer);
            console.log("Got back RTC answer.");
            await rtcConn.setRemoteDescription(remoteDesc);
            break;

        case CANDIDATE:
            const candidateJson = JSON.parse(msg.data);
            const candidate = new RTCIceCandidate(candidateJson);
            await rtcConn.addIceCandidate(candidate);
            break;

        default:
            console.log('Found unexpected "action" in message: ' + event.data);
            break;
    }
}

/// Construct a JSON message that can be sent over the websocket.
function message(action, data) {
    if (typeof data !== "undefined") {
        return '{ "action": "' + action + '", "data": "' + data + '" }';
    } else {
        return '{ "action": "' + action + '" }';
    }
}
