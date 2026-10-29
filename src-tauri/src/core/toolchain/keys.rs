//! Release signing keys compiled into the app. See `keys/README.md` for sources,
//! fingerprints and the rotation procedure.

/// A pinned OpenPGP public key (primary key fingerprint, upper-case hex).
#[derive(Debug, Clone, Copy)]
pub struct PinnedKey {
    pub owner: &'static str,
    pub fingerprint: &'static str,
    pub armored: &'static str,
}

macro_rules! key {
    ($owner:expr, $fpr:literal, $file:literal) => {
        PinnedKey {
            owner: $owner,
            fingerprint: $fpr,
            armored: include_str!(concat!("../../../keys/", $file)),
        }
    };
}

pub const HASHICORP: &[PinnedKey] = &[key!(
    "HashiCorp Security",
    "C874011F0AB405110D02105534365D9472D7468F",
    "hashicorp-C874011F0AB405110D02105534365D9472D7468F.asc"
)];

pub const OPENTOFU: &[PinnedKey] = &[key!(
    "OpenTofu",
    "E3E6E43D84CB852EADB0051D0C0AF313E5FD9F80",
    "opentofu-E3E6E43D84CB852EADB0051D0C0AF313E5FD9F80.asc"
)];

/// Active Node.js release keys (nodejs/release-keys `gpg-only-active-keys`).
pub const NODE: &[PinnedKey] = &[
    key!(
        "Node.js (Antoine du Hamel)",
        "5BE8A3F6C8A5C01D106C0AD820B1A390B168D356",
        "node/5BE8A3F6C8A5C01D106C0AD820B1A390B168D356.asc"
    ),
    key!(
        "Node.js (Juan José Arboleda)",
        "DD792F5973C6DE52C432CBDAC77ABFA00DDBF2B7",
        "node/DD792F5973C6DE52C432CBDAC77ABFA00DDBF2B7.asc"
    ),
    key!(
        "Node.js (Marco Ippolito)",
        "CC68F5A3106FF448322E48ED27F5E38D5B0A215F",
        "node/CC68F5A3106FF448322E48ED27F5E38D5B0A215F.asc"
    ),
    key!(
        "Node.js (Rafael Gonzaga)",
        "890C08DB8579162FEE0DF9DB8BEAB4DFCF555EF4",
        "node/890C08DB8579162FEE0DF9DB8BEAB4DFCF555EF4.asc"
    ),
    key!(
        "Node.js (Richard Lau)",
        "C82FA3AE1CBEDC6BE46B9360C43CEC45C17AB93C",
        "node/C82FA3AE1CBEDC6BE46B9360C43CEC45C17AB93C.asc"
    ),
    key!(
        "Node.js (Ruy Adorno)",
        "108F52B48DB57BB0CC439B2997B01419BD92F80A",
        "node/108F52B48DB57BB0CC439B2997B01419BD92F80A.asc"
    ),
    key!(
        "Node.js (Stewart X Addison)",
        "655F3B5C1FB3FA8D1A0CA6BDE4A7D232B936D2FD",
        "node/655F3B5C1FB3FA8D1A0CA6BDE4A7D232B936D2FD.asc"
    ),
    key!(
        "Node.js (Ulises Gascón)",
        "A363A499291CBBC940DD62E41F10027AF002F8B0",
        "node/A363A499291CBBC940DD62E41F10027AF002F8B0.asc"
    ),
];
