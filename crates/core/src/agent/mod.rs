pub mod builtin;
pub mod dispatcher;
pub mod registry;
pub mod router;
pub mod security;
pub mod types;

pub use dispatcher::{AgentDispatcher, SessionAffinity};
pub use registry::AgentRegistry;
pub use security::{CommandBlacklist, CommandCheckResult, PermissionMode, check_command};
pub use types::{
    Agent, AgentContext, AgentDescriptor, AgentEvent, AgentResult, Artifact, DynAgent,
};

use gpui::App;

pub fn init(cx: &mut App) {
    registry::init(cx);
}
