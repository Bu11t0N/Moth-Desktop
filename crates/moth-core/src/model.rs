use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Vless,
    Trojan,
}

impl Protocol {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Vless => "VLESS",
            Self::Trojan => "Trojan",
        }
    }
}

// Deliberately no Debug: profiles contain credentials.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: Uuid,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub protocol: Protocol,
    pub credential: String,
    pub tls: Tls,
    pub transport: Transport,
    pub flow: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Tls {
    None,
    Tls {
        server_name: String,
        fingerprint: Option<String>,
        alpn: Vec<String>,
    },
    Reality {
        server_name: String,
        fingerprint: String,
        public_key: String,
        short_id: String,
    },
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Transport {
    Tcp,
    Ws {
        path: String,
        host: Option<String>,
    },
    Grpc {
        service_name: String,
    },
    Xhttp {
        path: String,
        host: Option<String>,
        mode: String,
    },
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSummary {
    pub id: Uuid,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub protocol: String,
    pub transport: String,
    pub security: String,
}

impl Profile {
    pub fn summary(&self) -> ProfileSummary {
        ProfileSummary {
            id: self.id,
            name: self.name.clone(),
            host: self.host.clone(),
            port: self.port,
            protocol: self.protocol.label().into(),
            transport: match self.transport {
                Transport::Tcp => "TCP",
                Transport::Ws { .. } => "WebSocket",
                Transport::Grpc { .. } => "gRPC",
                Transport::Xhttp { .. } => "XHTTP",
            }
            .into(),
            security: match self.tls {
                Tls::None => "Без TLS",
                Tls::Tls { .. } => "TLS",
                Tls::Reality { .. } => "Reality",
            }
            .into(),
        }
    }
    pub fn same_server(&self, other: &Self) -> bool {
        self.host == other.host
            && self.port == other.port
            && self.protocol == other.protocol
            && self.credential == other.credential
            && self.tls == other.tls
            && self.transport == other.transport
            && self.flow == other.flow
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    pub profiles: Vec<Profile>,
    pub selected: Option<Uuid>,
}

impl Library {
    pub fn add(&mut self, profiles: Vec<Profile>) -> usize {
        let before = self.profiles.len();
        for profile in profiles {
            if !self.profiles.iter().any(|p| p.same_server(&profile)) {
                self.profiles.push(profile);
            }
        }
        if self.selected.is_none() {
            self.selected = self.profiles.first().map(|p| p.id);
        }
        self.profiles.len() - before
    }
    pub fn select(&mut self, id: Uuid) -> bool {
        if !self.profiles.iter().any(|p| p.id == id) {
            return false;
        }
        self.selected = Some(id);
        true
    }
    pub fn remove(&mut self, id: Uuid) -> bool {
        let before = self.profiles.len();
        self.profiles.retain(|p| p.id != id);
        if self.selected == Some(id) {
            self.selected = self.profiles.first().map(|p| p.id);
        }
        before != self.profiles.len()
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.profiles.len() > 1000 {
            return Err("Слишком много серверов");
        }
        let mut ids = std::collections::HashSet::new();
        for p in &self.profiles {
            if !ids.insert(p.id)
                || p.port == 0
                || p.host.is_empty()
                || p.name.chars().count() > 160
                || p.credential.is_empty()
            {
                return Err("Некорректное хранилище профилей");
            }
        }
        if self.selected.is_some_and(|id| !ids.contains(&id)) {
            return Err("Выбранный сервер отсутствует");
        }
        Ok(())
    }
}

/// Build a configuration for validation by Xray, NOT a system-wide VPN.
/// No listener is exposed to the LAN; raw configs never cross the UI boundary.
pub fn xray_check_config(profile: &Profile) -> serde_json::Value {
    use serde_json::json;
    let settings = match profile.protocol {
        Protocol::Vless => {
            let mut user = json!({"id": profile.credential, "encryption":"none"});
            if let Some(flow) = &profile.flow {
                user["flow"] = json!(flow);
            }
            json!({"vnext":[{"address":profile.host,"port":profile.port,"users":[user]}]})
        }
        Protocol::Trojan => {
            json!({"servers":[{"address":profile.host,"port":profile.port,"password":profile.credential}]})
        }
    };
    let mut stream = json!({});
    match &profile.transport {
        Transport::Tcp => stream["network"] = json!("tcp"),
        Transport::Ws { path, host } => {
            stream["network"] = json!("ws");
            stream["wsSettings"] = json!({"path":path});
            if let Some(h) = host {
                stream["wsSettings"]["host"] = json!(h);
            }
        }
        Transport::Grpc { service_name } => {
            stream["network"] = json!("grpc");
            stream["grpcSettings"] = json!({"serviceName":service_name});
        }
        Transport::Xhttp { path, host, mode } => {
            stream["network"] = json!("xhttp");
            stream["xhttpSettings"] = json!({"path":path,"mode":mode});
            if let Some(h) = host {
                stream["xhttpSettings"]["host"] = json!(h);
            }
        }
    }
    match &profile.tls {
        Tls::None => stream["security"] = json!("none"),
        Tls::Tls {
            server_name,
            fingerprint,
            alpn,
        } => {
            stream["security"] = json!("tls");
            stream["tlsSettings"] = json!({"serverName":server_name,"allowInsecure":false});
            if let Some(fp) = fingerprint {
                stream["tlsSettings"]["fingerprint"] = json!(fp);
            }
            if !alpn.is_empty() {
                stream["tlsSettings"]["alpn"] = json!(alpn);
            }
        }
        Tls::Reality {
            server_name,
            fingerprint,
            public_key,
            short_id,
        } => {
            stream["security"] = json!("reality");
            stream["realitySettings"] = json!({"serverName":server_name,"fingerprint":fingerprint,"publicKey":public_key,"shortId":short_id});
        }
    }
    json!({"log":{"loglevel":"none"},"outbounds":[{"tag":"proxy","protocol":match profile.protocol {Protocol::Vless=>"vless",Protocol::Trojan=>"trojan"},"settings":settings,"streamSettings":stream}]})
}
