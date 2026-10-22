//! The signed release manifest (tf3-mods' approach, `companion-core` mod_signature.rs / updater.rs): a text with one
//! way of being written, Ed25519-signed on Paulo's PC by `scripts/release.sh` with a key that is in no repository.
//! Its public half is compiled in here (and in `app/desktop/release-signing.pub`, which a test holds it to), so
//! GitHub, or anything between it and the app, can withhold a release and nothing else: files the author did not
//! sign are refused before they are installed.
//!
//! ```text
//! shroudtopia release v2
//! version 0.4.3
//! app 0.1.1
//! file Shroudtopia.exe <size> <sha256>
//! file shroudtopia-mods-v2.zip <size> <sha256>
//! ```
//!
//! v2 is the mods zip with folders in it (`mods/minimap_mod/...`, EML's `dbghelp.dll`). Apps up to 0.1.5 read only
//! v1 (`shroudtopia-release.json`, `shroudtopia-mods.zip`, `mods/<name>.dll` and nothing else) and install the mods
//! before they swap themselves, so a zip they refuse would keep them on the old app for good: every release carries
//! both, the v1 pair with the mods those apps know, and this app reads only the v2 pair.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ed25519_dalek::{Signature, VerifyingKey};

/// The public half of the release key (its 32 bytes, base64); the private half is
/// `%USERPROFILE%\.shroudtopia\release-signing.key` on Paulo's PC. Another key here is another app.
pub const PRODUCT_KEY: &str = include_str!("../../../release-signing.pub");

pub const HEADER: &str = "shroudtopia release v2";
pub const EXE_ASSET: &str = "Shroudtopia.exe";
pub const MODS_ASSET: &str = "shroudtopia-mods-v2.zip";
pub const MANIFEST_ASSET: &str = "shroudtopia-release-v2.json";
pub const MAX_ASSET_BYTES: u64 = 256 * 1024 * 1024;

pub const BAD_SIGNATURE: &str = "a atualização não tem a assinatura do autor";
pub const BAD_MANIFEST: &str = "a atualização assinada não está no formato que este app conhece";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedFile {
    pub name: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// the mods' version (the release tag without `v`)
    pub version: String,
    /// the app's version in this release
    pub app: String,
    pub exe: SignedFile,
    pub mods: SignedFile,
}

pub fn is_version(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    (2..=4).contains(&parts.len()) && parts.iter().all(|p| !p.is_empty() && p.len() <= 6 && p.bytes().all(|b| b.is_ascii_digit()) && (p == &"0" || !p.starts_with('0')))
}

pub fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn number(s: &str) -> Option<u64> {
    (!s.is_empty() && s.len() <= 16 && s.bytes().all(|b| b.is_ascii_digit()) && (s == "0" || !s.starts_with('0'))).then(|| s.parse().ok()).flatten()
}

impl Manifest {
    pub fn text(&self) -> String {
        format!(
            "{HEADER}\nversion {}\napp {}\nfile {} {} {}\nfile {} {} {}\n",
            self.version, self.app, self.exe.name, self.exe.size, self.exe.sha256, self.mods.name, self.mods.size, self.mods.sha256
        )
    }

    fn parse(text: &str) -> Option<Manifest> {
        if text.len() > 2048 {
            return None;
        }
        let mut lines = text.strip_suffix('\n')?.split('\n');
        if lines.next()? != HEADER {
            return None;
        }
        let version = lines.next()?.strip_prefix("version ").filter(|v| is_version(v))?.to_string();
        let app = lines.next()?.strip_prefix("app ").filter(|v| is_version(v))?.to_string();
        let mut file = |name: &str| -> Option<SignedFile> {
            let mut w = lines.next()?.strip_prefix("file ")?.split(' ');
            let (n, size, sha) = (w.next()?, number(w.next()?)?, w.next()?);
            (w.next().is_none() && n == name && (1..=MAX_ASSET_BYTES).contains(&size) && is_sha256(sha)).then(|| SignedFile { name: n.into(), size, sha256: sha.into() })
        };
        let exe = file(EXE_ASSET)?;
        let mods = file(MODS_ASSET)?;
        if lines.next().is_some() {
            return None;
        }
        let m = Manifest { version, app, exe, mods };
        (m.text() == text).then_some(m)
    }
}

pub fn signature_holds(payload: &str, signature: &str, key: &str) -> bool {
    let Some(key) = STANDARD.decode(key.trim()).ok().and_then(|b| <[u8; 32]>::try_from(b).ok()) else { return false };
    let Ok(key) = VerifyingKey::from_bytes(&key) else { return false };
    let Some(sig) = STANDARD.decode(signature.trim()).ok().and_then(|b| <[u8; 64]>::try_from(b).ok()) else { return false };
    key.verify_strict(payload.as_bytes(), &Signature::from_bytes(&sig)).is_ok()
}

/// The manifest, when `signature` is `key`'s over the exact bytes of `payload` and the payload is a manifest.
pub fn verify(payload: &str, signature: &str, key: &str) -> Result<Manifest, &'static str> {
    if !signature_holds(payload, signature, key) {
        return Err(BAD_SIGNATURE);
    }
    Manifest::parse(payload).ok_or(BAD_MANIFEST)
}

/// `0.4.10` > `0.4.9`.
pub fn newer(a: &str, b: &str) -> bool {
    let parse = |v: &str| v.trim_start_matches('v').split('.').map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    let (a, b) = (parse(a), parse(b));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

#[cfg(test)]
pub mod testing {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    pub const SEED: [u8; 32] = [7; 32];

    pub fn test_key() -> String {
        STANDARD.encode(SigningKey::from_bytes(&SEED).verifying_key().to_bytes())
    }

    pub fn sign(payload: &str) -> String {
        STANDARD.encode(SigningKey::from_bytes(&SEED).sign(payload.as_bytes()).to_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;

    const SHA: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

    fn text() -> String {
        format!("{HEADER}\nversion 0.4.3\napp 0.1.1\nfile Shroudtopia.exe 5 {SHA}\nfile shroudtopia-mods-v2.zip 5 {SHA}\n")
    }

    /// The same text signed by `openssl pkeyutl -sign -rawin` (what release.sh runs) with a throwaway key: the
    /// signatures the script makes are the ones this crate checks.
    #[test]
    fn an_openssl_signature_verifies() {
        let key = "7/+l4w5vHZfRzrt70ELCAs//C7SJmvx6XUs0BW+vlQI=";
        let sig = "a+6NJz1SzuxnLSLnIgMpellVQ1qqgirfGvls/t5iSssOou+d9sDfhT8o1JrfoU2k2IHwmrfqiFsT6AbxMh+OCw==";
        let m = verify(&text(), sig, key).unwrap();
        assert_eq!((m.version.as_str(), m.app.as_str(), m.exe.size, m.mods.name.as_str()), ("0.4.3", "0.1.1", 5, MODS_ASSET));
        assert_eq!(verify(&text(), sig, PRODUCT_KEY).err(), Some(BAD_SIGNATURE));
        assert_eq!(verify(&text().replace("0.4.3", "0.4.4"), sig, key).err(), Some(BAD_SIGNATURE));
    }

    #[test]
    fn the_product_key_is_a_key_and_not_the_test_one() {
        assert_eq!(STANDARD.decode(PRODUCT_KEY.trim()).unwrap().len(), 32);
        assert_ne!(PRODUCT_KEY.trim(), test_key());
    }

    #[test]
    fn only_the_one_way_of_writing_it() {
        let good = text();
        assert!(verify(&good, &sign(&good), &test_key()).is_ok());
        for bad in [
            good.replace(HEADER, "shroudtopia release v1"),
            good.replace(MODS_ASSET, "shroudtopia-mods.zip"),
            good.trim_end().to_string(),
            format!("{good}\n"),
            good.replace('\n', "\r\n"),
            good.replace("version 0.4.3", "version 0.4.03"),
            good.replace("version 0.4.3", "version v0.4.3"),
            good.replace("Shroudtopia.exe", "../Shroudtopia.exe"),
            good.replace(" 5 ", " 0 "),
            good.replace(SHA, &SHA.to_uppercase()),
            good.replace("app 0.1.1\n", ""),
        ] {
            assert_eq!(verify(&bad, &sign(&bad), &test_key()).err(), Some(BAD_MANIFEST), "{bad:?}");
        }
    }

    #[test]
    fn versions_compare_as_numbers() {
        assert!(newer("0.4.10", "0.4.9"));
        assert!(newer("v1.0.0", "0.9.9"));
        assert!(!newer("0.4.3", "0.4.3"));
        assert!(!newer("0.4.2", "0.4.3"));
    }
}
