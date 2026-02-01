use crate::CatacombClient;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Client {
    pub address: String,
    pub class: String,
    pub title: String,
    pub workspace: Workspace,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: i32,
    pub name: String,
}

pub fn get_clients() -> Vec<Client> {
    CatacombClient::get_clients()
        .into_iter()
        .map(|c| Client {
            address: c.app_id.clone(), // Using app_id as address
            class: c.app_id.clone(),
            title: c.title,
            workspace: Workspace {
                id: 1,
                name: "1".to_string(),
            },
        })
        .collect()
}

pub fn focus_window(address: &str) {
    CatacombClient::focus_window(address);
}
