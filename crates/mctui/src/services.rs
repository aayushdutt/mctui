//! The online services screens talk to, shared across the app.

use mctui_core::auth::Auth;
use mctui_core::fabric::Fabric;
use mctui_core::modrinth;
use mctui_core::mojang::Mojang;
use mctui_core::net::Http;
use mctui_core::paths::DataDir;
use mctui_core::vanilla_tweaks::VanillaTweaks;

/// Clients for every online service. Cheap to clone; tests point them at a
/// mock server.
#[derive(Debug, Clone)]
pub struct Services {
    pub http: Http,
    pub auth: Auth,
    pub mojang: Mojang,
    pub fabric: Fabric,
    pub modrinth: modrinth::Client,
    pub vanilla_tweaks: VanillaTweaks,
}

impl Services {
    pub fn new(http: Http, data: &DataDir) -> Self {
        Self {
            auth: Auth::new(http.clone()),
            mojang: Mojang::new(http.clone(), data),
            fabric: Fabric::new(http.clone(), data),
            modrinth: modrinth::Client::new(http.clone()),
            vanilla_tweaks: VanillaTweaks::new(http.clone(), data),
            http,
        }
    }
}
