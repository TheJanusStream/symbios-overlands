//! Tool-role catalogue entries — utility items personalised at build
//! time (the teleporter stamps the local user's DID). The social-gateway
//! placeholder that once lived here was retired in #749-772 once every
//! theme grew a bespoke gateway; the cross-theme fallback is now
//! `civic::gateway::CivicGateway`.

pub mod my_teleporter;
