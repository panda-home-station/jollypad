pub mod shell;
pub mod pad;
pub mod ipc;
pub mod clients;
pub mod catacomb_client;
pub mod game_launcher;

// Re-export common types if needed
pub use pad::get_default_items as get_pad_items;
pub use ipc::{HyprlandListener, HyprEvent};
pub use catacomb_client::CatacombClient;
pub use catacomb_ipc::ClientInfo;
