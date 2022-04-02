var webSocket;
var rtcConn;

/// The remote host might start sending ICE candidates before we have received the
/// remove description. We can not register the candidates for the connection
/// before the remote description have been registered, so we need to buffer the
/// candidates until the remote description have been received and registered.
var candidates_buffer = [];

/// URL of STUN server to use when setting up WebRTC connection.
const STUN_URL = "stun:stun.l.google.com:19302";

/// Values set in the `action` field indicating what type of message that is
/// being sent over the websocket connection.
const OFFER = "offer";
const ANSWER = "answer";
const CANDIDATE = "candidate";
const RESPONSE = "response";

function connect() {
    if (!("WebSocket" in window)) {
        return alert("Websockets not supported in this browser.");
    } else if (typeof webSocket !== "undefined" && webSocket != null) {
        return alert("Already connected.");
    }

    showStatusMessage("Setting up websocket connection...");

    let localWebSocket;
    try {
        const url = document.getElementById("url").value;
        console.log("Connecting to URL: " + url);
        localWebSocket = new WebSocket(url);
    } catch (e) {
        hideStatusMessage();
        const msg = "Unable to connect to URL: " + url.value + "\n"
                  + "Make sure that the URL is formatted correctly (must include protocol).";
        return alert(msg);
    }

    localWebSocket.onopen = function (_) {
        hideStatusMessage();
        webSocket = localWebSocket;
        sendRtcOffer();
    };

    localWebSocket.onclose = function (_) {
        hideStatusMessage();
        showConnect();
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
    hideStatusMessage();
    showConnect();
    if (typeof webSocket !== "undefined") {
        webSocket.close();
        webSocket = null;
    }
    if (typeof rtcConn !== "undefined") {
        rtcConn.close();
        rtcConn = null;
    }
}

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
        const video = document.getElementById("video");
        video.srcObject = event.streams[0];
    };

    rtcConn.onicecandidate = event => {
        if (event.candidate) {
            const candidate = JSON.stringify(event.candidate.toJSON());
            console.log("Sending ICE candidate");
            webSocket.send(message(CANDIDATE, candidate));
        }
    };

    rtcConn.oniceconnectionstatechange = _ => {
        console.log("ICE Connection State changed: " + rtcConn.iceConnectionState);
        if (rtcConn.iceConnectionState === "connected") {
            showDisconnect();
            hideStatusMessage();
        }
    };
}

async function sendRtcOffer() {
    try {
        showStatusMessage("Sending RTC offer...");
        rtcConn.addTransceiver('video', { 'direction': 'recvonly' });
        const localDesc = await rtcConn.createOffer();
        await rtcConn.setLocalDescription(localDesc);
        const offer = JSON.stringify(rtcConn.localDescription.toJSON());
        console.log("Sending RTC offer");
        webSocket.send(message(OFFER, offer));
    } catch (error) {
        showStatusMessage("Unable to send WebRTC offer: " + error);
    }
}

/// Reads & handles the data received from the server over the websocket.
async function onmessageHandler(event) {
    const msg = JSON.parse(event.data);
    switch (msg.action) {
        case OFFER:
            console.log("ERROR - Should never get OFFER from server");
            break;

        case ANSWER:
            showStatusMessage("Received RTC answer, exchanging candidates...");
            console.log("Received RTC answer");
            const remoteDesc = new RTCSessionDescription(msg.data);
            await rtcConn.setRemoteDescription(remoteDesc);
            for (let i = 0; i < candidates_buffer.length; i++) {
                const candidate = candidates_buffer[i];
                await rtcConn.addIceCandidate(candidate);
            }
            break;

        case CANDIDATE:
            console.log("Received ICE candidate");
            const candidate = new RTCIceCandidate(msg.data);
            if (rtcConn.remoteDescription != null) {
                await rtcConn.addIceCandidate(candidate);
            } else {
                candidates_buffer.push(candidate);
            }
            break;

        case RESPONSE:
            if (msg.type == "offer") {
                showStatusMessage("RTC offer received on other end, negotiating candidates...");
            }
            console.log('Received response from signaling server: ' + event.data);
            break;

        default:
            console.log('Received message without action: ' + event.data);
            break;
    }
}

/// Construct a JSON message that can be sent over the websocket. The given data
/// should already be a formatted json string.
function message(action, data) {
    if (typeof data !== "undefined") {
        return '{ "action": "' + action + '", "data": ' + data + ' }';
    } else {
        return '{ "action": "' + action + '" }';
    }
}

function showConnect() {
    document.getElementById("connect").style.display = "block";
    document.getElementById("disconnect").style.display = "none";
}

function showDisconnect() {
    document.getElementById("connect").style.display = "none";
    document.getElementById("disconnect").style.display = "block";
}

function showStatusMessage(msg) {
    updateStatusMessage(msg, "block");
}

function hideStatusMessage() {
    updateStatusMessage("", "none");
}

function updateStatusMessage(msg, display) {
    const content = document.getElementById("status");
    content.innerHTML = msg;
    content.style.display = display;
}
