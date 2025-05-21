use serde::{Deserialize, Serialize};
use webrtc::{
    ice_transport::ice_candidate::RTCIceCandidateInit,
    peer_connection::sdp::session_description::RTCSessionDescription,
};

use crate::ConnectionId;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateMessage {
    pub id: String,
    pub action: String,
    pub data: RTCIceCandidateInit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Message {
    Answer {
        id: ConnectionId,
        data: RTCSessionDescription,
    },
    Candidate {
        id: ConnectionId,
        data: RTCIceCandidateInit,
    },
    Disconnect {
        id: ConnectionId,
    },
    Offer {
        id: ConnectionId,
        data: RTCSessionDescription,
    },
    Response {
        r#type: String,
        data: String,
    },
}
