// ============================================================
// 微信 IPC — 数据域门面（各职责域已下沉）
// ============================================================

mod contacts;
mod favorites;
mod general;
mod media;
mod moments;
mod overview;
mod paths;
mod revoked;
mod status;
mod storage;
pub use contacts::*;
pub use favorites::*;
pub use general::*;
pub use media::*;
pub use moments::*;
pub use overview::*;
pub use paths::*;
pub use revoked::*;
pub use status::*;
pub use storage::*;
