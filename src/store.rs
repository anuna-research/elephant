//! Theory store (SPEC-001 ADR-002, REQ-003/REQ-004, CON-005).
//!
//! One Loro doc per theory; single list container `"corpus"` holding Entry
//! JSON strings, insert-only. The store is the effectful shell around the
//! pure core: it owns files, clocks tick here, and closure inputs are read
//! out as plain `Vec<Entry>`.

use crate::core::envelope::{Entry, Hlc, SpeechAct};
use crate::errors::{AppError, AppResult};
use crate::id::Identity;
use crate::paths::Paths;
use loro::{ExportMode, LoroDoc};
use serde::{Deserialize, Serialize};

pub const CORPUS_CONTAINER: &str = "corpus";
/// Theory field value carried by a genesis entry (the id doesn't exist
/// until the genesis bytes are hashed).
pub const GENESIS_THEORY: &str = "genesis";

#[derive(Debug, Serialize, Deserialize)]
pub struct TheoryMeta {
    pub theory_id: String,
    pub alias: String,
    pub created: String,
}

pub struct TheoryStore {
    pub theory_id: String,
    pub meta: TheoryMeta,
    doc: LoroDoc,
    paths: Paths,
    /// Corpus data keys (SPEC-004 REQ-303). Present for every theory this
    /// agent is a member of; absent only for a corpus we cannot decrypt.
    keybook: Option<crate::e2ee::keybook::Keybook>,
}

/// did-crdt's node-id binding: low 64 bits (LE) of blake3(pubkey).
pub fn node_id_for(ident: &Identity) -> u64 {
    did_crdt::core::validate::node_id_from_pubkey(ident.signing_key.verifying_key().as_bytes())
}

impl TheoryStore {
    /// REQ-003: mint genesis, derive theory id, initialise the doc.
    pub fn create(
        paths: &Paths,
        ident: &Identity,
        name: &str,
        wall_ms: u64,
        ts_rfc3339: &str,
    ) -> AppResult<TheoryStore> {
        // Refuse alias collision (HP-O6 failure mode).
        if resolve_alias(paths, name)?.is_some() {
            return Err(AppError::Config(format!(
                "a theory with alias '{name}' already exists"
            )));
        }

        let node_id = node_id_for(ident);
        let hlc = Hlc {
            wall_ms,
            logical: 0,
            node_id,
        };
        let sid = Entry::sentence_id(GENESIS_THEORY, ident.did.as_str(), hlc);
        let spl = format!(
            "(meta theory (name \"{}\") (creator \"{}\") (created \"{}\"))",
            escape(name),
            ident.did.as_str(),
            ts_rfc3339
        );
        let genesis = Entry::create(
            GENESIS_THEORY,
            hlc,
            ident.did.as_str(),
            &format!("{}#key-0", ident.did.as_str()),
            &SpeechAct::Assert {
                sentence_id: sid,
                spl,
            },
            ts_rfc3339,
            &ident.signing_key,
        );
        let genesis_json = crate::core::envelope::entry_to_json(&genesis);
        let theory_id = blake3::hash(genesis_json.as_bytes()).to_hex().to_string();

        let dir = paths.theory_dir(&theory_id);
        std::fs::create_dir_all(dir.join("members"))?;

        // SPEC-004 REQ-301/REQ-303: mint the MLS group (creator = steward)
        // and the genesis keybook before anything is written.
        let provider = crate::e2ee::open_provider(paths, &theory_id)?;
        let mls_ident = crate::e2ee::MlsIdentity::for_theory(ident, &theory_id);
        crate::e2ee::create_group(&provider, &mls_ident, &theory_id)?;
        let (keybook, k0) = crate::e2ee::keybook::Keybook::genesis();
        keybook.save(&crate::e2ee::keybook::keybook_path(paths, &theory_id))?;

        let sealed = crate::e2ee::seal::seal(&genesis, &theory_id, 0, &k0)?;
        let doc = LoroDoc::new();
        doc.set_peer_id(node_id)
            .map_err(|e| AppError::Internal(format!("loro peer id: {e}")))?;
        doc.get_list(CORPUS_CONTAINER)
            .push(crate::e2ee::seal::to_json(&sealed).as_str())
            .map_err(|e| AppError::Internal(format!("loro push: {e}")))?;
        doc.commit();

        let meta = TheoryMeta {
            theory_id: theory_id.clone(),
            alias: name.to_string(),
            created: ts_rfc3339.to_string(),
        };
        std::fs::write(
            paths.theory_meta(&theory_id),
            toml::to_string_pretty(&meta).map_err(|e| AppError::Internal(e.to_string()))?,
        )?;

        // The creator is the first member: persist our DID document.
        let member_doc = ident
            .document
            .to_bytes()
            .map_err(|e| AppError::Internal(format!("did doc: {e}")))?;
        std::fs::write(
            dir.join("members")
                .join(format!("{}.json", ident.did.method_specific_id())),
            member_doc,
        )?;

        let store = TheoryStore {
            theory_id,
            meta,
            doc,
            paths: paths.clone(),
            keybook: Some(keybook),
        };
        store.flush()?;
        Ok(store)
    }

    /// Open by id or alias.
    pub fn open(paths: &Paths, id_or_alias: &str) -> AppResult<TheoryStore> {
        let theory_id = if paths.theory_dir(id_or_alias).is_dir() {
            id_or_alias.to_string()
        } else {
            resolve_alias(paths, id_or_alias)?.ok_or_else(|| {
                AppError::NotFound(format!("theory '{id_or_alias}' (no such id or alias)"))
            })?
        };
        let meta: TheoryMeta =
            toml::from_str(&std::fs::read_to_string(paths.theory_meta(&theory_id))?)
                .map_err(|e| AppError::Config(format!("theory meta corrupt: {e}")))?;
        let doc = LoroDoc::new();
        let snapshot = std::fs::read(paths.theory_doc(&theory_id))?;
        doc.import(&snapshot)
            .map_err(|e| AppError::Config(format!("corpus corrupt: {e}")))?;
        let keybook = crate::e2ee::keybook::Keybook::load(&crate::e2ee::keybook::keybook_path(
            paths, &theory_id,
        ))?;
        Ok(TheoryStore {
            theory_id,
            meta,
            doc,
            paths: paths.clone(),
            keybook,
        })
    }

    /// Materialise a theory learned from a join ceremony (SPEC-002 REQ-104):
    /// no genesis minting, no MLS group creation — the keybook and the MLS
    /// group already exist from the Welcome. Called only after the ceremony
    /// has authenticated the steward, so nothing partial is left on failure.
    pub fn adopt(
        paths: &Paths,
        ident: &Identity,
        theory_id: &str,
        alias: &str,
        keybook: crate::e2ee::keybook::Keybook,
        steward_did: &str,
        steward_did_doc: &[u8],
    ) -> AppResult<TheoryStore> {
        if let Some(existing) = resolve_alias(paths, alias)? {
            if existing != theory_id {
                return Err(AppError::Config(format!(
                    "local alias '{alias}' already names a different theory"
                )));
            }
        }
        let dir = paths.theory_dir(theory_id);
        std::fs::create_dir_all(dir.join("members"))?;
        keybook.save(&crate::e2ee::keybook::keybook_path(paths, theory_id))?;

        let meta = TheoryMeta {
            theory_id: theory_id.to_string(),
            alias: alias.to_string(),
            created: chrono::Utc::now().to_rfc3339(),
        };
        std::fs::write(
            paths.theory_meta(theory_id),
            toml::to_string_pretty(&meta).map_err(|e| AppError::Internal(e.to_string()))?,
        )?;

        // Both members' DID documents, so every signature verifies offline.
        let members = dir.join("members");
        let tail = |did: &str| did.rsplit(':').next().unwrap_or(did).to_string();
        std::fs::write(
            members.join(format!("{}.json", tail(steward_did))),
            steward_did_doc,
        )?;
        std::fs::write(
            members.join(format!("{}.json", ident.did.method_specific_id())),
            ident
                .document
                .to_bytes()
                .map_err(|e| AppError::Internal(format!("did doc: {e}")))?,
        )?;

        let doc = LoroDoc::new();
        doc.set_peer_id(node_id_for(ident))
            .map_err(|e| AppError::Internal(format!("loro peer id: {e}")))?;
        Ok(TheoryStore {
            theory_id: theory_id.to_string(),
            meta,
            doc,
            paths: paths.clone(),
            keybook: Some(keybook),
        })
    }

    /// The underlying Loro doc, for sync sessions (SPEC-002 CON-103).
    pub fn doc(&self) -> &LoroDoc {
        &self.doc
    }

    /// Persist after an external mutation of the doc (e.g. a sync import).
    pub fn flush_public(&self) -> AppResult<()> {
        self.flush()
    }

    /// Set the local peer id before local writes (unique per identity).
    pub fn bind_identity(&self, ident: &Identity) -> AppResult<()> {
        self.doc
            .set_peer_id(node_id_for(ident))
            .map_err(|e| AppError::Internal(format!("loro peer id: {e}")))?;
        Ok(())
    }

    /// Rotate the theory's corpus data key (SPEC-004 REQ-305): mint the next
    /// keybook generation and persist it. Subsequent `append_batch` seals
    /// under the new generation, so a member removed from the MLS group
    /// (who therefore never receives the new keybook) cannot read anything
    /// written after this point. Existing entries stay readable via the
    /// earlier generations already in the keybook (ADR-302).
    pub fn rotate_keybook(&mut self) -> AppResult<()> {
        let kb = self.keybook.as_mut().ok_or_else(|| {
            AppError::Config("no keybook for this theory (are you a member?)".into())
        })?;
        kb.rotate();
        kb.save(&crate::e2ee::keybook::keybook_path(
            &self.paths,
            &self.theory_id,
        ))
    }

    /// All corpus entries, deserialised, in deterministic (hlc, signer)
    /// order. Undeserialisable elements are returned separately — they are
    /// quarantine candidates, never dropped silently (REQ-016).
    pub fn entries(&self) -> (Vec<Entry>, Vec<(usize, String)>) {
        let list = self.doc.get_list(CORPUS_CONTAINER);
        let mut entries = Vec::new();
        let mut malformed = Vec::new();
        for (i, v) in list
            .get_value()
            .into_list()
            .unwrap_or_default()
            .iter()
            .enumerate()
        {
            let Some(s) = v.as_string() else {
                malformed.push((i, "non-string corpus element".to_string()));
                continue;
            };
            // REQ-303: every element is a SealedEntry; opening is fail-closed.
            let sealed = match crate::e2ee::seal::from_json(s) {
                Ok(sealed) => sealed,
                Err(err) => {
                    malformed.push((i, err.to_string()));
                    continue;
                }
            };
            let keys = |g: u32| self.keybook.as_ref().and_then(|kb| kb.key(g));
            match crate::e2ee::seal::open(&sealed, &self.theory_id, &keys) {
                Ok(e) => entries.push(e),
                Err(err) => malformed.push((i, err.to_string())),
            }
        }
        entries.sort_by(|a, b| {
            a.hlc
                .cmp(&b.hlc)
                .then_with(|| a.signer.cmp(&b.signer))
                .then_with(|| a.sig.cmp(&b.sig))
        });
        entries.dedup_by(|a, b| a.sig == b.sig && a.cbcl == b.cbcl);
        (entries, malformed)
    }

    /// Next HLC for a local append: monotone over everything we've seen.
    pub fn tick(&self, ident: &Identity, wall_ms: u64) -> Hlc {
        let node_id = node_id_for(ident);
        let (entries, _) = self.entries();
        let max_seen = entries.iter().map(|e| e.hlc).max();
        match max_seen {
            Some(last) if wall_ms <= last.wall_ms => Hlc {
                wall_ms: last.wall_ms,
                logical: last.logical + 1,
                node_id,
            },
            _ => Hlc {
                wall_ms,
                logical: 0,
                node_id,
            },
        }
    }

    /// Append a batch of locally-signed entries in one commit (bundles,
    /// SPEC-003 ADR-202).
    ///
    /// Single-writer discipline (SPEC-002 REQ-102): all writers — the
    /// daemon, a direct-mode CLI append, and the join ceremony — take an
    /// exclusive per-theory file lock and re-import the latest on-disk
    /// snapshot before appending, so concurrent writers merge rather than
    /// clobber. Loro's import is an idempotent CRDT merge, so re-reading is
    /// safe even mid-session.
    pub fn append_batch(&self, entries: &[Entry]) -> AppResult<()> {
        let kb = self.keybook.as_ref().ok_or_else(|| {
            AppError::Config(format!(
                "no keybook for theory {} — cannot seal (are you a member?)",
                self.theory_id
            ))
        })?;
        let key = kb
            .current_key()
            .ok_or_else(|| AppError::Config("keybook has no current key".into()))?;

        let _guard = self.write_lock()?;
        // Fold in anything another writer committed since we opened.
        if let Ok(snapshot) = std::fs::read(self.paths.theory_doc(&self.theory_id)) {
            let _ = self.doc.import(&snapshot);
        }
        let list = self.doc.get_list(CORPUS_CONTAINER);
        for entry in entries {
            let sealed = crate::e2ee::seal::seal(entry, &self.theory_id, kb.current, &key)?;
            list.push(crate::e2ee::seal::to_json(&sealed).as_str())
                .map_err(|e| AppError::Internal(format!("loro push: {e}")))?;
        }
        self.doc.commit();
        self.flush()
    }

    /// Exclusive per-theory write lock, held for one append (REQ-102).
    fn write_lock(&self) -> AppResult<std::fs::File> {
        use fs2::FileExt as _;
        let dir = self.paths.theory_dir(&self.theory_id);
        std::fs::create_dir_all(&dir)?;
        let f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(dir.join("write.lock"))?;
        f.lock_exclusive()
            .map_err(|e| AppError::Transport(format!("theory write lock: {e}")))?;
        Ok(f)
    }

    /// Append a locally-signed entry and persist (REQ-020: no network).
    pub fn append(&self, entry: &Entry) -> AppResult<()> {
        self.append_batch(std::slice::from_ref(entry))
    }

    /// The theory's keybook, when this agent is a member (SPEC-004).
    pub fn keybook(&self) -> Option<&crate::e2ee::keybook::Keybook> {
        self.keybook.as_ref()
    }

    /// Known member DID documents → key resolver for merge validation.
    pub fn key_resolver(
        &self,
    ) -> impl Fn(&str, &str) -> Option<ed25519_dalek::VerifyingKey> + use<> {
        let mut docs: Vec<did_crdt::DidDocument> = Vec::new();
        let members_dir = self.paths.theory_dir(&self.theory_id).join("members");
        if let Ok(rd) = std::fs::read_dir(&members_dir) {
            for f in rd.flatten() {
                if let Ok(bytes) = std::fs::read(f.path()) {
                    if let Ok(doc) = did_crdt::Document::from_bytes(&bytes) {
                        if let Ok(res) = doc.resolve() {
                            if let Some(dd) = res.did_document {
                                docs.push(dd);
                            }
                        }
                    }
                }
            }
        }
        move |did: &str, key_id: &str| {
            use base64ct::{Base64UrlUnpadded, Encoding as _};
            for dd in &docs {
                if dd.id != did {
                    continue;
                }
                for vm in &dd.verification_method {
                    if vm.id != key_id {
                        continue;
                    }
                    let mb = &vm.public_key_multibase;
                    let b64 = mb.strip_prefix('u')?;
                    let bytes = Base64UrlUnpadded::decode_vec(b64).ok()?;
                    let arr = <[u8; 32]>::try_from(bytes.as_slice()).ok()?;
                    return ed25519_dalek::VerifyingKey::from_bytes(&arr).ok();
                }
            }
            None
        }
    }

    /// The expected `theory` field for validation: genesis entries carry
    /// the sentinel, all others the real id.
    pub fn expected_theory_for(&self, entry: &Entry) -> &str {
        if entry.theory == GENESIS_THEORY {
            GENESIS_THEORY
        } else {
            &self.theory_id
        }
    }

    fn flush(&self) -> AppResult<()> {
        let bytes = self
            .doc
            .export(ExportMode::Snapshot)
            .map_err(|e| AppError::Internal(format!("loro export: {e}")))?;
        let path = self.paths.theory_doc(&self.theory_id);
        let tmp = path.with_extension("loro.tmp");
        std::fs::write(&tmp, &bytes)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// REQ-004: list all theories with entry counts.
pub fn list_theories(paths: &Paths) -> AppResult<Vec<(TheoryMeta, usize)>> {
    let mut out = Vec::new();
    let dir = paths.theories_dir();
    if !dir.is_dir() {
        return Ok(out);
    }
    for d in std::fs::read_dir(dir)?.flatten() {
        if !d.path().is_dir() {
            continue;
        }
        let id = d.file_name().to_string_lossy().to_string();
        if let Ok(store) = TheoryStore::open(paths, &id) {
            let (entries, malformed) = store.entries();
            out.push((store.meta, entries.len() + malformed.len()));
        }
    }
    out.sort_by(|a, b| a.0.alias.cmp(&b.0.alias));
    Ok(out)
}

fn resolve_alias(paths: &Paths, alias: &str) -> AppResult<Option<String>> {
    let dir = paths.theories_dir();
    if !dir.is_dir() {
        return Ok(None);
    }
    for d in std::fs::read_dir(dir)?.flatten() {
        let meta_path = d.path().join("meta.toml");
        if let Ok(text) = std::fs::read_to_string(&meta_path) {
            if let Ok(meta) = toml::from_str::<TheoryMeta>(&text) {
                if meta.alias == alias {
                    return Ok(Some(meta.theory_id));
                }
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, Paths, Identity) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths {
            home: dir.path().to_path_buf(),
        };
        let ident = crate::id::create(&paths, Some("alice".into())).unwrap();
        (dir, paths, ident)
    }

    /// TEST-003: create → genesis verifies, id = blake3(genesis bytes).
    #[test]
    fn create_theory_and_reload() {
        let (_d, paths, ident) = setup();
        let store = TheoryStore::create(
            &paths,
            &ident,
            "release-v3",
            1_752_000_000_000,
            "2026-07-11T00:00:00Z",
        )
        .unwrap();
        assert_eq!(store.theory_id.len(), 64);

        let reopened = TheoryStore::open(&paths, "release-v3").unwrap();
        assert_eq!(reopened.theory_id, store.theory_id);
        let (entries, malformed) = reopened.entries();
        assert!(malformed.is_empty());
        assert_eq!(entries.len(), 1);

        // Genesis verifies against the creator's key via the member docs.
        let genesis = &entries[0];
        assert_eq!(
            blake3::hash(crate::core::envelope::entry_to_json(genesis).as_bytes())
                .to_hex()
                .to_string(),
            store.theory_id,
            "tampered genesis would change the theory id"
        );
        let resolve = reopened.key_resolver();
        let act = crate::core::envelope::validate_entry(
            genesis,
            reopened.expected_theory_for(genesis),
            &resolve,
        )
        .unwrap();
        assert_eq!(act.performative(), "assert");
    }

    /// TEST-003 negative-input: duplicate alias refused.
    #[test]
    fn duplicate_alias_refused() {
        let (_d, paths, ident) = setup();
        TheoryStore::create(&paths, &ident, "t", 1, "2026-07-11T00:00:00Z").unwrap();
        assert!(TheoryStore::create(&paths, &ident, "t", 2, "2026-07-11T00:00:00Z").is_err());
    }

    /// TEST-004: listing shows created theories with counts.
    #[test]
    fn list_shows_theories() {
        let (_d, paths, ident) = setup();
        TheoryStore::create(&paths, &ident, "a", 1, "2026-07-11T00:00:00Z").unwrap();
        TheoryStore::create(&paths, &ident, "b", 2, "2026-07-11T00:00:00Z").unwrap();
        let all = list_theories(&paths).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].0.alias, "a");
        assert_eq!(all[0].1, 1);
    }

    /// HLC ticks are strictly monotone even with a stuck wall clock.
    #[test]
    fn hlc_monotone() {
        let (_d, paths, ident) = setup();
        let store = TheoryStore::create(&paths, &ident, "t", 1000, "2026-07-11T00:00:00Z").unwrap();
        let h1 = store.tick(&ident, 1000);
        assert!(h1.logical > 0 || h1.wall_ms > 1000); // genesis used wall 1000
        let e = Entry::create(
            &store.theory_id,
            h1,
            ident.did.as_str(),
            &format!("{}#key-0", ident.did.as_str()),
            &SpeechAct::Assert {
                sentence_id: "s-x".into(),
                spl: "(given x)".into(),
            },
            "2026-07-11T00:00:01Z",
            &ident.signing_key,
        );
        store.append(&e).unwrap();
        let h2 = store.tick(&ident, 500); // wall clock went backwards
        assert!(h2 > h1, "hlc must be monotone: {h2:?} vs {h1:?}");
    }
}
