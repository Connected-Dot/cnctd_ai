use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Auth {
    Bearer { token: String },
    Oauth2 { client_id: String, client_secret: String },
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connector {
    pub name: String,
    pub description: Option<String>,
    pub url: String,
    pub auth: Auth,
}

impl Connector {
    pub fn new<N, U, D>(name: N, url: U, auth: Auth, description: Option<D>) -> Self
    where
        N: Into<String>,
        U: Into<String>,
        D: Into<String>,
    {
        Self {
            name: name.into(),
            url: url.into(),
            auth,
            description: description.map(|d| d.into()),
        }
    }
}