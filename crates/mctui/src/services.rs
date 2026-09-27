//! The online services screens talk to, shared across the app.

use mctui_core::auth::Auth;
use mctui_core::net::Http;

/// Clients for every online service. Cheap to clone; tests point them at a
/// mock server.
#[derive(Debug, Clone)]
#[expect(dead_code, reason = "the accounts, wizard, mods and packs screens use them")]
pub struct Services {
    pub http: Http,
    pub auth: Auth,
}

impl Services {
    pub fn new(http: Http) -> Self {
        Self {
            auth: Auth::new(http.clone()),
            http,
        }
    }
}
