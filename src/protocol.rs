use serde::{Deserialize, Serialize};

/// All messages exchanged over the P2P network.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum OverlandsMessage {
    /// Physics transform broadcast at ~60 Hz over the Unreliable channel.
    Transform {
        position: [f32; 3],
        rotation: [f32; 4],
    },
    /// Reliable identity announcement sent on join and periodically thereafter.
    Identity { did: String, handle: String },
    /// Chat message sent over the Reliable channel.
    Chat { text: String },
}
