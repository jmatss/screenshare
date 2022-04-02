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
    pub data: RTCIceCandidateInitFix,
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
        data: RTCIceCandidateInitFix,
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

// TODO: Remove when new release of WebRTC (RTCIceCandidateInit is fixed in
//       that release).
// TODO: Firefox are sending a `sdpMLineIndex` as a float value??
/// Fix property names for RTCIceCandidateInit.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RTCIceCandidateInitFix {
    pub candidate: String,
    pub sdp_mid: Option<String>,
    //#[serde(rename = "sdpMLineIndex")]
    //pub sdp_mline_index: Option<u16>,
    pub username_fragment: Option<String>,
}

impl RTCIceCandidateInitFix {
    pub fn fix_from(init: RTCIceCandidateInit) -> Self {
        Self {
            candidate: init.candidate,
            // TODO: Atleast one of the `sdp_mid` or `sdp_nline_index` must be
            //       set according to the javascript implementation of WebSocket.
            //       This is incorrect according to the specs, but must send
            //       some value to make the implementation happy.
            /*
            sdp_mid: if !init.sdp_mid.is_empty() {
                Some(init.sdp_mid)
            } else {
                None
            },
            */
            sdp_mid: Some(0.to_string()),
            //sdp_mline_index: Some(init.sdp_mline_index),
            username_fragment: if !init.username_fragment.is_empty() {
                Some(init.username_fragment)
            } else {
                None
            },
        }
    }

    pub fn fix_to(self) -> RTCIceCandidateInit {
        RTCIceCandidateInit {
            candidate: self.candidate,
            sdp_mid: if let Some(sdp_mid) = self.sdp_mid {
                sdp_mid
            } else {
                "".into()
            },
            /*
            sdp_mline_index: if let Some(sdp_mline_index) = self.sdp_mline_index {
                sdp_mline_index
            } else {
                0
            },
            */
            sdp_mline_index: 0,
            username_fragment: if let Some(username_fragment) = self.username_fragment {
                username_fragment
            } else {
                "".into()
            },
        }
    }
}
