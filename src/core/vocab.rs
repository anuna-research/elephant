//! SPEC-005 vocabulary layer (pure core): family resolution (CON-402),
//! the built-in registry (REQ-404), documentation records (REQ-402/REQ-407),
//! the vocabulary view (REQ-401/CON-403) and the near-miss advisory
//! computation (REQ-406).
//!
//! Everything here is a projection over an already-computed `Closure` —
//! nothing influences admission, conclusions, or commitment states
//! (ADR-404). Family identity composes spindle's SPEC-024 predicate model
//! (`PredicateSymbol`, `Vocabulary::derive`, `MetaTarget::Predicate`);
//! elephant adds only the legacy suffix resolver for the frozen flat
//! vocabulary, the `goal` role (commitments/requests, which spindle has no
//! concept of), and the proven co-body witness join that grounds
//! `hole`/sibling demand (ADR-402).

use crate::core::closure::Closure;
use crate::core::envelope::SpeechAct;
use spindle_core::grounding::{Substitution, apply_substitution_to_literal, match_literal};
use spindle_core::literal::Literal;
use spindle_core::rule::RuleType;
use spindle_core::theory::{Meta, MetaValue, Theory};
use spindle_core::vocabulary::{
    HasPredicateSymbol, OccurrenceRole, PredicateSymbol, Vocabulary, VocabularyDiagnostic,
    is_variable_symbol,
};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// Synthetic commitment-trigger rule label prefix (see `core::closure`).
const CT_PREFIX: &str = "__ct-";
/// Synthetic trigger-literal prefix (see `core::closure` TRIG_PREFIX).
const TRIG_PREFIX: &str = "__trig-";

/// Is this name one of the closure's own synthetic constructs? These are
/// exactly the `__ct-<sid>` rule labels and `__trig-<sid>` literals the
/// closure generates — never the whole `__*` namespace, which the
/// admission grammar does not reserve: an admitted user functor such as
/// `__user` is legal vocabulary that status and closure expose, so the
/// view must expose it too (REQ-401).
fn synthetic_name(name: &str) -> bool {
    name.starts_with(CT_PREFIX) || name.starts_with(TRIG_PREFIX)
}

// ── Family (CON-402) ────────────────────────────────────────────────────

/// A predicate family — a discriminated identity, never a bare string.
/// The three constructors occupy disjoint key spaces even when they render
/// to the same bytes (CON-402): a parameterised `(p x)` is
/// `Predicate(p/1)`, a flat atom literally spelled `p/1` is
/// `Legacy("p/1")`, and a functor spindle cannot form a symbol from is
/// `Malformed(<raw>)` — distinct malformed functors stay distinct.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Family {
    Predicate(PredicateSymbol),
    Legacy(String),
    Malformed(String),
}

impl Family {
    pub fn kind(&self) -> &'static str {
        match self {
            Family::Predicate(_) => "predicate",
            Family::Legacy(_) => "legacy",
            Family::Malformed(_) => "malformed",
        }
    }

    fn kind_rank(&self) -> u8 {
        match self {
            Family::Predicate(_) => 0,
            Family::Legacy(_) => 1,
            Family::Malformed(_) => 2,
        }
    }

    /// The rendered family key (CON-403 `family`): the predicate indicator
    /// `functor/arity`, the flat stem, or the escaped raw functor.
    pub fn rendered(&self) -> String {
        match self {
            Family::Predicate(sym) => sym.indicator().to_string(),
            Family::Legacy(stem) => stem.clone(),
            Family::Malformed(raw) => escape_controls(raw),
        }
    }

    /// The functor the built-in registry is keyed on (membership is by
    /// functor at every arity, REQ-404). Malformed families never match.
    pub fn registry_functor(&self) -> Option<String> {
        match self {
            Family::Predicate(sym) => Some(sym.functor().to_string()),
            Family::Legacy(stem) => Some(stem.clone()),
            Family::Malformed(_) => None,
        }
    }
}

impl Ord for Family {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // The malformed key must be the RAW functor, not the escaped
        // rendering: escaping is not injective, and an Ord that conflated
        // two raw functors with the same escaped form would break the
        // Eq/Ord contract and merge distinct Malformed families in the
        // view's BTreeMap (CON-402: never merged).
        let key = |f: &Family| match f {
            Family::Malformed(raw) => raw.clone(),
            other => other.rendered(),
        };
        (self.kind_rank(), key(self)).cmp(&(other.kind_rank(), key(other)))
    }
}

impl PartialOrd for Family {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Escape C0/C1 controls for display (§8 terminal-injection defence).
pub fn escape_controls(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if is_c0c1(c) {
            out.push_str(&format!("\\u{{{:04x}}}", c as u32));
        } else {
            out.push(c);
        }
    }
    out
}

fn is_c0c1(c: char) -> bool {
    c < '\u{20}' || c == '\u{7f}' || ('\u{80}'..='\u{9f}').contains(&c)
}

// ── Built-in registry (REQ-404) ─────────────────────────────────────────

pub struct BuiltinRow {
    pub functor: &'static str,
    pub kind: &'static str,
    pub description: &'static str,
}

/// The closed built-in registry (REQ-404). Normative and exhaustive;
/// membership is by functor at every arity and covers the legacy flat
/// suffix family of each functor. Not extensible from the wire.
pub const BUILTINS: &[BuiltinRow] = &[
    BuiltinRow {
        functor: "task",
        kind: "control",
        description: "declared unit of work (legacy hence lifecycle)",
    },
    BuiltinRow {
        functor: "no-deps",
        kind: "control",
        description: "task has no dependencies (readiness root)",
    },
    BuiltinRow {
        functor: "ready",
        kind: "control",
        description: "task's dependencies are met; it may be claimed",
    },
    BuiltinRow {
        functor: "completed",
        kind: "control",
        description: "task is finished (given fact; lifecycle terminal)",
    },
    BuiltinRow {
        functor: "claimed",
        kind: "control",
        description: "an agent has taken ownership of the task",
    },
    BuiltinRow {
        functor: "blocked",
        kind: "control",
        description: "task is blocked by a recorded impediment",
    },
    BuiltinRow {
        functor: "upstream-blocked",
        kind: "control",
        description: "a dependency of the task is blocked",
    },
    BuiltinRow {
        functor: "decomposed",
        kind: "control",
        description: "task was split into sub-tasks",
    },
    BuiltinRow {
        functor: "permanently-failed",
        kind: "control",
        description: "task failed with no retry path",
    },
    BuiltinRow {
        functor: "assign-to",
        kind: "control",
        description: "task is assigned to the named agent",
    },
    BuiltinRow {
        functor: "agent-available",
        kind: "control",
        description: "the named agent is available for assignment",
    },
    BuiltinRow {
        functor: "claim",
        kind: "control",
        description: "versioned lifecycle action on the task",
    },
    BuiltinRow {
        functor: "unclaim",
        kind: "control",
        description: "versioned lifecycle action on the task",
    },
    BuiltinRow {
        functor: "block",
        kind: "control",
        description: "versioned lifecycle action on the task",
    },
    BuiltinRow {
        functor: "unblock",
        kind: "control",
        description: "versioned lifecycle action on the task",
    },
    BuiltinRow {
        functor: "state-claimed",
        kind: "control",
        description: "versioned lifecycle state of the task",
    },
    BuiltinRow {
        functor: "state-unclaimed",
        kind: "control",
        description: "versioned lifecycle state of the task",
    },
    BuiltinRow {
        functor: "state-blocked",
        kind: "control",
        description: "versioned lifecycle state of the task",
    },
    BuiltinRow {
        functor: "state-unblocked",
        kind: "control",
        description: "versioned lifecycle state of the task",
    },
    BuiltinRow {
        functor: "stale",
        kind: "control",
        description: "versioned staleness/timeout signal on the task",
    },
    BuiltinRow {
        functor: "timeout",
        kind: "control",
        description: "versioned staleness/timeout signal on the task",
    },
    BuiltinRow {
        functor: "commitment-state",
        kind: "control",
        description: "current state of a commitment (SPEC-001 REQ-026)",
    },
    BuiltinRow {
        functor: "failed",
        kind: "control",
        description: "work failed (evidence signal; consumed by propagation rules)",
    },
    BuiltinRow {
        functor: "discovered",
        kind: "discovery",
        description: "a fact an agent discovered during work",
    },
    BuiltinRow {
        functor: "decided",
        kind: "discovery",
        description: "a decision an agent recorded",
    },
    BuiltinRow {
        functor: "blocked-by",
        kind: "discovery",
        description: "records what blocks progress",
    },
    BuiltinRow {
        functor: "requires",
        kind: "discovery",
        description: "records a prerequisite an agent found",
    },
    BuiltinRow {
        functor: "verified",
        kind: "discovery",
        description: "evidence a task's work is verified (RECOMMENDED evidence-rule conclusion)",
    },
    BuiltinRow {
        functor: "finding",
        kind: "discovery",
        description: "a finding recorded during work",
    },
    BuiltinRow {
        functor: "approach",
        kind: "discovery",
        description: "an approach an agent recorded",
    },
    BuiltinRow {
        functor: "insight",
        kind: "discovery",
        description: "an insight recorded during work",
    },
    BuiltinRow {
        functor: "partial",
        kind: "discovery",
        description: "a partial result recorded during work",
    },
];

pub fn builtin(functor: &str) -> Option<&'static BuiltinRow> {
    BUILTINS.iter().find(|b| b.functor == functor)
}

/// Versioned-action families (`<family>-v<N>-<task>` ground pattern).
const VERSIONED: &[&str] = &[
    "state-unclaimed",
    "state-unblocked",
    "state-claimed",
    "state-blocked",
    "unclaim",
    "unblock",
    "timeout",
    "claim",
    "block",
    "stale",
];

/// Suffix families (`<family>-<suffix>` ground pattern), longest first so
/// `upstream-blocked-x` never resolves to `blocked`, `blocked-by-x` never
/// to `blocked`, `permanently-failed-x` never to `failed`.
const SUFFIXED: &[&str] = &[
    "permanently-failed",
    "upstream-blocked",
    "blocked-by",
    "discovered",
    "decomposed",
    "assign-to",
    "completed",
    "requires",
    "verified",
    "approach",
    "no-deps",
    "insight",
    "claimed",
    "blocked",
    "finding",
    "partial",
    "decided",
    "failed",
    "ready",
    "task",
];

/// CON-402 rule 2: match a flat atom against the closed built-in ground
/// patterns. Unlisted names (`deploy-v3-m1`) match nothing here.
fn builtin_ground_match(atom: &str) -> Option<&'static str> {
    for fam in VERSIONED {
        if let Some(rest) = atom.strip_prefix(fam).and_then(|r| r.strip_prefix("-v")) {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            if digits > 0 && rest.len() > digits + 1 && rest.as_bytes()[digits] == b'-' {
                return Some(fam);
            }
        }
    }
    if let Some(mid) = atom
        .strip_prefix("agent-")
        .and_then(|s| s.strip_suffix("-available"))
        && !mid.is_empty()
    {
        return Some("agent-available");
    }
    for fam in SUFFIXED {
        if let Some(rest) = atom.strip_prefix(fam).and_then(|r| r.strip_prefix('-'))
            && !rest.is_empty()
        {
            return Some(fam);
        }
    }
    None
}

// ── Family resolution (CON-402) ─────────────────────────────────────────

/// Resolve a literal to its family. Deterministic, total, a function of
/// exactly these two arguments — never of documentation (ADR-402).
/// Negation is ignored (families are positive).
pub fn family(lit: &Literal, tasks: &BTreeSet<String>) -> Family {
    if !lit.predicate_args().is_empty() {
        // Rule 1: arity ≥ 1 → spindle's PredicateSymbol; Err (empty or
        // control-character functor) → Malformed, never a panic or drop.
        match lit.predicate_symbol() {
            Ok(sym) => Family::Predicate(sym),
            Err(_) => Family::Malformed(lit.name().to_string()),
        }
    } else if PredicateSymbol::validate_functor(lit.name()).is_err() {
        // Arity 0 with an empty or control-character functor: spindle's
        // `Vocabulary::derive` keys this occurrence as `MalformedPredicate`
        // (arity 0), so `family` must agree — else the demand/advisory path
        // keys `Legacy("")` while the row is `Malformed("")` and they never
        // join (a valid `(always r "" done)` mis-classes the family).
        Family::Malformed(lit.name().to_string())
    } else {
        flat_family(lit.name(), tasks)
    }
}

/// CON-402 rules 2–4 for a flat (arity-0) atom — the legacy path kept for
/// the frozen hence vocabulary and pre-predicate corpora.
pub fn flat_family(atom: &str, tasks: &BTreeSet<String>) -> Family {
    // Rule 2: built-in ground patterns.
    if let Some(fam) = builtin_ground_match(atom) {
        return Family::Legacy(fam.to_string());
    }
    // Rule 3: longest declared-task suffix preceded by `-`.
    let mut best: Option<&str> = None;
    for t in tasks {
        if atom.len() > t.len() + 1
            && atom.ends_with(t.as_str())
            && atom.as_bytes()[atom.len() - t.len() - 1] == b'-'
            && best.is_none_or(|b| t.len() > b.len())
        {
            best = Some(t);
        }
    }
    if let Some(t) = best {
        return Family::Legacy(atom[..atom.len() - t.len() - 1].to_string());
    }
    // Rule 4: the atom is its own family.
    Family::Legacy(atom.to_string())
}

// ── Proven facts (REQ-401 definition) ───────────────────────────────────

/// The accepted weighted-conclusion set, indexed for the witness join.
/// "Proven" throughout REQ-401/REQ-406 means exactly membership here: a
/// positive-tag (+D/+d) conclusion of the closure's weighted set, under
/// the local trust policy and evaluation time (SPEC-001 REQ-023). The tag
/// already reflects trust weighting where the policy defines any (this is
/// the same provability notion commitment fulfilment uses); a degree
/// filter would empty the set on the default policy, whose default trust
/// is zero.
pub struct ProvenSet {
    keys: HashSet<(bool, String)>,
    by_shape: HashMap<(bool, String, usize), Vec<Literal>>,
}

fn lit_key(l: &Literal) -> (bool, String) {
    (l.negation, l.to_spl())
}

impl ProvenSet {
    pub fn from_closure(closure: &Closure) -> ProvenSet {
        let mut keys = HashSet::new();
        let mut by_shape: HashMap<(bool, String, usize), Vec<Literal>> = HashMap::new();
        for w in &closure.weighted {
            if w.conclusion_type.is_positive() {
                let l = &w.literal;
                if keys.insert(lit_key(l)) {
                    by_shape
                        .entry((l.negation, l.name().to_string(), l.predicate_args().len()))
                        .or_default()
                        .push(l.clone());
                }
            }
        }
        ProvenSet { keys, by_shape }
    }

    pub fn contains(&self, l: &Literal) -> bool {
        self.keys.contains(&lit_key(l))
    }

    fn candidates(&self, pattern: &Literal) -> &[Literal] {
        self.by_shape
            .get(&(
                pattern.negation,
                pattern.name().to_string(),
                pattern.predicate_args().len(),
            ))
            .map_or(&[], Vec::as_slice)
    }
}

/// The declared-task set (CON-402 `tasks`): task names X declared by an
/// admitted fact rule `(given task-X)` whose `task-X` atom is a proven
/// conclusion of this closure pass. Both legs are required — CON-402
/// says "not the rule-head set": a `task-X` atom that is merely the
/// derived conclusion of a non-fact rule is present in the proven set
/// but is NOT a declaration, and must not split flat families.
pub fn provable_tasks(theory: &Theory, proven: &ProvenSet) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (_, rule) in theory.rules_with_labels() {
        if rule.rule_type != RuleType::Fact {
            continue;
        }
        for head in &rule.head {
            if head.negation || !head.predicate_args().is_empty() {
                continue;
            }
            if let Some(task) = head.name().strip_prefix("task-")
                && !task.is_empty()
                && proven.contains(head)
            {
                out.insert(task.to_string());
            }
        }
    }
    out
}

// ── Documentation records (REQ-402, CON-401, REQ-407) ───────────────────

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FamilyDoc {
    pub description: Option<String>,
    pub kind: Option<String>,
    pub asserter: Option<String>,
    /// Non-conforming key names, byte-lexicographic (CON-403).
    pub malformed: Vec<String>,
    /// Signer DID of the latest conforming `description` write (REQ-407).
    pub documenter: Option<String>,
    /// ≥ 2 distinct signers wrote conforming descriptions (REQ-407).
    pub redefined: bool,
    /// Documentation joins no occurrence-projected family (REQ-401).
    pub detached: bool,
}

impl FamilyDoc {
    pub fn is_documented(&self) -> bool {
        self.description.is_some()
    }
}

const DOC_KEYS: [&str; 3] = ["description", "kind", "asserter"];

fn conforming_string(v: &MetaValue, max_bytes: usize) -> Option<String> {
    if let MetaValue::String(s) = v
        && (1..=max_bytes).contains(&s.len())
        && !s.chars().any(is_c0c1)
    {
        return Some(s.clone());
    }
    None
}

/// Does a candidate string conform to CON-401 for the given key? Used by
/// `define` to recognise argv values before signing (REQ-403).
pub fn value_conforms(key: &str, value: &str) -> bool {
    conforming_value(key, &MetaValue::String(value.to_string())).is_some()
}

fn conforming_value(key: &str, v: &MetaValue) -> Option<String> {
    match key {
        "description" => conforming_string(v, 512),
        "asserter" => conforming_string(v, 128),
        "kind" => conforming_string(v, 64)
            .filter(|s| matches!(s.as_str(), "evidence" | "state" | "discovery")),
        _ => None,
    }
}

/// Evaluate a merged metadata record per key (REQ-402): each CON-401 key
/// independently; a non-conforming key is surfaced as `malformed` and
/// treated as absent — never repaired, never poisoning the other keys.
/// Returns None when the record carries no CON-401 key at all (metadata
/// on non-family targets is out of scope).
fn doc_from_meta(meta: &Meta) -> Option<FamilyDoc> {
    let mut doc = FamilyDoc::default();
    let mut any = false;
    for key in DOC_KEYS {
        if let Some(v) = meta.properties.get(key) {
            any = true;
            match conforming_value(key, v) {
                Some(s) => match key {
                    "description" => doc.description = Some(s),
                    "kind" => doc.kind = Some(s),
                    "asserter" => doc.asserter = Some(s),
                    _ => unreachable!(),
                },
                None => doc.malformed.push(key.to_string()),
            }
        }
    }
    doc.malformed.sort();
    any.then_some(doc)
}

/// A documentation write surfaced from one admitted assert's payload: the
/// family targeted, the CON-401 key, and whether its value conforms. The
/// single recognizer behind both REQ-407 surfaces — the view's provenance
/// pass (which keeps conforming `description` writes) and `redefinitions`
/// (which counts any key write). Keeping one scanner means the prefilter and
/// the synthetic/builtin/rule-label exclusions cannot drift between them.
struct DocWrite {
    family: Family,
    key: String,
    conforming: bool,
}

/// Scan one admitted assert's SPL payload for the CON-401 documentation
/// writes it carries (REQ-402/REQ-407). Empty for a payload carrying no doc
/// key. `theory` supplies the rule-label check for the legacy carrier.
fn doc_writes(theory: &Theory, spl: &str) -> Vec<DocWrite> {
    // Cheap prefilter only — it must be a superset of what the parser
    // accepts: SPL allows whitespace after `(`, so gating on "(meta" would
    // let `( meta …)` redefine documentation invisibly (adversarial-review
    // finding #1); and SPL quoted atoms unescape `\X` → `X` before keyword
    // dispatch, so `("me\ta" …)` IS a meta form whose source lacks the bare
    // byte substring — every such spelling necessarily carries a backslash,
    // so an escape-free payload without either keyword can never write docs.
    if !spl.contains("meta") && !spl.contains("predicate") && !spl.contains('\\') {
        return Vec::new();
    }
    let Ok(t2) = spindle_parser::parse_spl(spl) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (sym, meta) in t2.predicate_metadata() {
        let functor = sym.functor().to_string();
        if synthetic_name(&functor) || builtin(&functor).is_some() {
            continue;
        }
        for key in DOC_KEYS {
            if let Some(v) = meta.properties.get(key) {
                out.push(DocWrite {
                    family: Family::Predicate(*sym),
                    key: key.to_string(),
                    conforming: conforming_value(key, v).is_some(),
                });
            }
        }
    }
    for (label, meta) in t2.metadata() {
        if synthetic_name(label) || theory.get_rule(label).is_some() || builtin(label).is_some() {
            continue;
        }
        for key in DOC_KEYS {
            if let Some(v) = meta.properties.get(key) {
                out.push(DocWrite {
                    family: Family::Legacy(label.clone()),
                    key: key.to_string(),
                    conforming: conforming_value(key, v).is_some(),
                });
            }
        }
    }
    out
}

// ── Roles, classes, rows (REQ-401 / CON-403) ────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Fact,
    Head,
    Body,
    Goal,
}

impl Role {
    pub fn name(self) -> &'static str {
        match self {
            Role::Fact => "fact",
            Role::Head => "head",
            Role::Body => "body",
            Role::Goal => "goal",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Hole,
    Orphan,
    Active,
}

impl Class {
    pub fn name(self) -> &'static str {
        match self {
            Class::Hole => "hole",
            Class::Orphan => "orphan",
            Class::Active => "active",
        }
    }
}

#[derive(Clone, Debug)]
pub struct VocabRow {
    pub family: Family,
    pub roles: BTreeSet<Role>,
    pub class: Class,
    pub built_in: bool,
    pub doc: FamilyDoc,
}

/// A demanded-but-unproven ground instance (REQ-401 demand), attributable
/// to the rule label or commitment/request id that listens for it.
#[derive(Clone, Debug)]
pub struct Demand {
    pub family: Family,
    pub literal: Literal,
    pub listener: String,
}

pub struct VocabView {
    pub rows: Vec<VocabRow>,
    /// Unproven demanded instances across all families (advisory pool).
    pub demands: Vec<Demand>,
    /// The declared-task set the view was resolved under.
    pub tasks: BTreeSet<String>,
}

/// Ground-literal spelling for display and JSON: the corpus spelling —
/// parameterised `(ci-green m1)` or legacy flat `ci-green-m1`.
pub fn display_literal(l: &Literal) -> String {
    let base = if l.predicate_args().is_empty() {
        l.name().to_string()
    } else {
        l.to_spl()
    };
    if l.negation {
        format!("(not {base})")
    } else {
        base
    }
}

// ── The view (REQ-401) ──────────────────────────────────────────────────

pub fn view(closure: &Closure) -> VocabView {
    let proven = ProvenSet::from_closure(closure);
    let theory = &closure.theory;
    let tasks = provable_tasks(theory, &proven);

    struct Accum {
        roles: BTreeSet<Role>,
        doc: Option<FamilyDoc>,
        occurs: bool,
    }
    fn touch(fams: &mut BTreeMap<Family, Accum>, fam: Family) -> &mut Accum {
        fams.entry(fam).or_insert(Accum {
            roles: BTreeSet::new(),
            doc: None,
            occurs: false,
        })
    }
    let mut fams: BTreeMap<Family, Accum> = BTreeMap::new();

    // 1. Occurrence roles composed from spindle's Vocabulary::derive
    //    (head/body), re-grouping arity-0 symbols through the legacy
    //    resolver. Synthetic __ct- rules are elephant's own commitment
    //    encoding — their body occurrences are goal-role occurrences and
    //    are handled with the commitments below; __trig- heads are skipped.
    let report = Vocabulary::derive(theory);
    let handle_origin = |theory: &spindle_core::theory::Theory,
                         fams: &mut BTreeMap<Family, Accum>,
                         fam: &Family,
                         rule_label: &str,
                         role: &OccurrenceRole| {
        if rule_label.starts_with(CT_PREFIX) {
            if matches!(role, OccurrenceRole::Body { .. }) {
                let a = touch(fams, fam.clone());
                a.roles.insert(Role::Goal);
                a.occurs = true;
            }
            return;
        }
        let r = match role {
            OccurrenceRole::Head { .. } => {
                match theory.get_rule(rule_label).map(|ru| ru.rule_type) {
                    Some(RuleType::Fact) => Role::Fact,
                    _ => Role::Head,
                }
            }
            OccurrenceRole::Body { .. } => Role::Body,
        };
        let a = touch(fams, fam.clone());
        a.roles.insert(r);
        a.occurs = true;
    };
    for entry in &report.vocabulary.entries {
        let functor = entry.symbol.functor().to_string();
        if synthetic_name(&functor) {
            continue;
        }
        let fam = if entry.symbol.arity() >= 1 {
            Family::Predicate(entry.symbol)
        } else {
            flat_family(&functor, &tasks)
        };
        for origin in entry.origins.iter() {
            handle_origin(theory, &mut fams, &fam, &origin.rule, &origin.role);
        }
    }
    for diag in &report.diagnostics {
        if let VocabularyDiagnostic::MalformedPredicate {
            functor, origins, ..
        } = diag
        {
            let fam = Family::Malformed(functor.clone());
            for origin in origins.iter() {
                handle_origin(theory, &mut fams, &fam, &origin.rule, &origin.role);
            }
        }
    }

    // 2. Goal roles + demand from commitments and requests (elephant-only:
    //    spindle's model has no commitments). Non-retracted admitted acts.
    let mut demands: Vec<Demand> = Vec::new();
    let mut seen_demand: HashSet<(Family, (bool, String), String)> = HashSet::new();
    let mut push_demand = |demands: &mut Vec<Demand>, fam: Family, lit: Literal, listener: &str| {
        let k = (fam.clone(), lit_key(&lit), listener.to_string());
        if seen_demand.insert(k) {
            demands.push(Demand {
                family: fam,
                literal: lit,
                listener: listener.to_string(),
            });
        }
    };

    // (listener, trigger, goal, is_request) per non-retracted commitment or
    // request. `is_request` is recorded here, from the same act match, so the
    // trigger-demand branch below needs no second O(admitted) scan of the
    // corpus (the former `listener_is_request` lookup was quadratic).
    let goal_exprs: Vec<(String, Option<String>, Option<String>, bool)> = closure
        .admitted
        .iter()
        .filter(|a| !a.retracted)
        .filter_map(|a| match &a.act {
            SpeechAct::Commit {
                sentence_id,
                trigger,
                goal,
                ..
            } => Some((
                sentence_id.clone(),
                Some(trigger.clone()),
                Some(goal.clone()),
                false,
            )),
            SpeechAct::Request {
                request_id,
                trigger,
                goal,
                ..
            } => Some((
                request_id.clone(),
                Some(trigger.clone()),
                Some(goal.clone()),
                true,
            )),
            _ => None,
        })
        .collect();
    for (listener, trigger, goal, is_request) in &goal_exprs {
        let mut goal_lits: Vec<BodyLit> = Vec::new();
        if let Some(t) = trigger
            && !t.trim().is_empty()
        {
            goal_lits.extend(parse_body_literals(t));
        }
        if let Some(g) = goal
            && !g.trim().is_empty()
            && let Some(l) = parse_one_literal(g)
        {
            goal_lits.push(BodyLit {
                joinable: lit_joinable(&l),
                lit: l,
            });
        }
        // Roles: every trigger/goal literal gives its family role `goal`.
        for l in &goal_lits {
            if synthetic_name(l.lit.name()) {
                continue;
            }
            let fam = family(&l.lit, &tasks);
            let a = touch(&mut fams, fam);
            a.roles.insert(Role::Goal);
            a.occurs = true;
        }
        // Demand: goal-side literals are in-fragment only when flat ground
        // (a ground parameterised goal has no co-body binder; a variable
        // goal has no witness) — REQ-401 out-of-fragment list. Commitment
        // *triggers* also ride the synthetic __ct- rules and get the full
        // body treatment in step 3; requests have no synthetic rule, so
        // their triggers are joined here.
        let request_trigger_lits: Vec<BodyLit> = if *is_request {
            trigger
                .as_deref()
                .filter(|t| !t.trim().is_empty())
                .map(parse_body_literals)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        if !request_trigger_lits.is_empty() {
            demand_from_body(
                &request_trigger_lits,
                listener,
                &proven,
                &tasks,
                &mut |fam, lit, listener| push_demand(&mut demands, fam, lit, listener),
            );
        }
        if let Some(g) = goal
            && let Some(l) = parse_one_literal(g)
            && lit_joinable(&l)
            && l.predicate_args().is_empty()
            && !l.negation
            && !synthetic_name(l.name())
            && !is_var_name(&l)
            && !proven.contains(&l)
        {
            let fam = family(&l, &tasks);
            if !matches!(fam, Family::Malformed(_)) {
                push_demand(&mut demands, fam, l, listener);
            }
        }
    }

    // 3. Demand from rule bodies (REQ-401 grounding and demand): flat body
    //    atoms unconditionally; variable templates via the proven co-body
    //    witness join; everything else contributes no demand (bounded
    //    named deferral). Synthetic __ct- rules carry commitment triggers —
    //    their listener is the commitment id.
    for (label, rule) in theory.rules_with_labels() {
        if rule.rule_type == RuleType::Fact {
            continue;
        }
        let listener = label
            .strip_prefix(CT_PREFIX)
            .map(str::to_string)
            .unwrap_or_else(|| label.to_string());
        if synthetic_name(label) && !label.starts_with(CT_PREFIX) {
            continue;
        }
        // Every logic literal stays in the body list (a dropped conjunct
        // would make the sibling join unsound and would wrongly strip a
        // flat atom's syntactic demand — adversarial-review finding #2);
        // arithmetic/temporal occurrences are carried but marked
        // unjoinable, so they contribute no demand themselves and block
        // any witness join that would need them (REQ-401 out-of-fragment).
        let body: Vec<BodyLit> = rule
            .body
            .iter()
            .filter_map(|b| b.as_logic())
            .map(|l| BodyLit {
                joinable: !l.has_arith_args() && !l.is_temporal() && !l.has_temporal_variables(),
                lit: l.to_literal(),
            })
            .collect();
        demand_from_body(&body, &listener, &proven, &tasks, &mut |fam, lit, l| {
            push_demand(&mut demands, fam, lit, l)
        });
    }

    // Unproven filter is applied inside demand_from_body; sort for
    // determinism (CON-403 orderings are applied again at render time).
    demands.sort_by(|a, b| {
        (&a.family, display_literal(&a.literal), &a.listener).cmp(&(
            &b.family,
            display_literal(&b.literal),
            &b.listener,
        ))
    });

    // 4. Documentation join (REQ-402): predicate-symbol carrier, then the
    //    legacy label carrier (rule labels win; built-ins frozen).
    for (sym, meta) in theory.predicate_metadata() {
        let functor = sym.functor().to_string();
        if synthetic_name(&functor) || builtin(&functor).is_some() {
            continue;
        }
        if let Some(doc) = doc_from_meta(meta) {
            let a = touch(&mut fams, Family::Predicate(*sym));
            a.doc = Some(doc);
        }
    }
    for (label, meta) in theory.metadata() {
        if synthetic_name(label) || theory.get_rule(label).is_some() || builtin(label).is_some() {
            continue;
        }
        if let Some(doc) = doc_from_meta(meta) {
            let a = touch(&mut fams, Family::Legacy(label.clone()));
            a.doc = Some(doc);
        }
    }

    // 5. Provenance (REQ-407): one pass over admitted Entries in canonical
    //    order — the merged metadata map alone carries no attribution. Only
    //    a conforming `description` write attributes a documenter.
    let mut prov: HashMap<Family, (String, BTreeSet<String>)> = HashMap::new();
    for a in &closure.admitted {
        if a.retracted || a.label_shadowed {
            continue;
        }
        let SpeechAct::Assert { spl, .. } = &a.act else {
            continue;
        };
        for w in doc_writes(theory, spl) {
            if w.key == "description" && w.conforming {
                let e = prov
                    .entry(w.family)
                    .or_insert_with(|| (a.entry.signer.clone(), BTreeSet::new()));
                e.0 = a.entry.signer.clone();
                e.1.insert(a.entry.signer.clone());
            }
        }
    }

    // 6. Assemble rows: classification, built-in marking, detached docs.
    let unproven_families: HashSet<Family> = demands.iter().map(|d| d.family.clone()).collect();
    let mut rows: Vec<VocabRow> = Vec::new();
    for (fam, acc) in &fams {
        let built = fam
            .registry_functor()
            .as_deref()
            .and_then(builtin)
            .is_some();
        let mut doc = if built {
            let b = builtin(fam.registry_functor().as_deref().unwrap()).unwrap();
            FamilyDoc {
                description: Some(b.description.to_string()),
                kind: Some(b.kind.to_string()),
                ..FamilyDoc::default()
            }
        } else {
            acc.doc.clone().unwrap_or_default()
        };
        if !built {
            if let Some((last, signers)) = prov.get(fam) {
                // CON-403: documenter is null on an undocumented family —
                // when the winning description is absent or malformed, the
                // historical conforming writer is not surfaced here.
                if doc.is_documented() {
                    doc.documenter = Some(last.clone());
                }
                doc.redefined = signers.len() >= 2;
            }
            doc.detached = !acc.occurs;
        }
        let has_listener = acc.roles.contains(&Role::Body) || acc.roles.contains(&Role::Goal);
        let heads_rule = acc.roles.contains(&Role::Head);
        let class = if has_listener && !heads_rule && unproven_families.contains(fam) {
            Class::Hole
        } else if acc.roles.len() == 1
            && acc.roles.contains(&Role::Fact)
            && !built
            && doc.kind.as_deref() != Some("discovery")
        {
            Class::Orphan
        } else {
            Class::Active
        };
        rows.push(VocabRow {
            family: fam.clone(),
            roles: acc.roles.clone(),
            class,
            built_in: built,
            doc,
        });
    }

    VocabView {
        rows,
        demands,
        tasks,
    }
}

/// A body literal for demand computation. `joinable` = the occurrence
/// sits in the REQ-401 supported fragment (no arithmetic argument
/// positions, no temporal binding); unjoinable literals carry roles but
/// contribute no demand and block any witness join that needs them.
struct BodyLit {
    lit: Literal,
    joinable: bool,
}

/// Parse an SPL body expression (a literal or an `(and …)` conjunction)
/// into its literals, via the single SPL recogniser.
fn parse_body_literals(expr: &str) -> Vec<BodyLit> {
    let Ok(t) =
        spindle_parser::parse_spl(&format!("(always __probe {} __probe-head)", expr.trim()))
    else {
        return Vec::new();
    };
    t.rules()
        .find(|r| r.label == "__probe")
        .map(|r| {
            r.body
                .iter()
                .filter_map(|b| b.as_logic())
                .map(|l| BodyLit {
                    joinable: !l.has_arith_args()
                        && !l.is_temporal()
                        && !l.has_temporal_variables(),
                    lit: l.to_literal(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_one_literal(expr: &str) -> Option<Literal> {
    let t = spindle_parser::parse_spl(&format!("(given {})", expr.trim())).ok()?;
    t.rules().next().and_then(|r| r.head.first().cloned())
}

/// Fragment membership for a standalone literal (commitment/request goal).
fn lit_joinable(l: &Literal) -> bool {
    l.temporal_expr.is_none() && l.interval_var.is_none() && l.temporal.is_empty()
}

fn is_var_name(l: &Literal) -> bool {
    l.name().starts_with('?')
}

fn term_vars(l: &Literal) -> HashSet<spindle_core::intern::SymbolId> {
    let mut out = HashSet::new();
    for t in l.predicate_args() {
        if let spindle_core::term::Term::Symbol(id) = t
            && is_variable_symbol(*id)
        {
            out.insert(*id);
        }
    }
    out
}

fn is_ground(l: &Literal) -> bool {
    !is_var_name(l) && term_vars(l).is_empty()
}

/// Merge two substitutions; None on a conflicting binding.
fn merge_subst(a: &Substitution, b: &Substitution) -> Option<Substitution> {
    let mut out = a.clone();
    for (k, v) in &b.terms {
        if let Some(existing) = out.terms.get(k) {
            if existing != v {
                return None;
            }
        } else {
            out.terms.insert(*k, v.clone());
        }
    }
    Some(out)
}

/// REQ-401 demanded ground instances for one body (rule body, or a request
/// trigger conjunction). Emits only *unproven* demanded instances. A flat
/// atom's demand is syntactic and never depends on its siblings; witness
/// joins require every participating sibling to be in-fragment.
fn demand_from_body(
    body: &[BodyLit],
    listener: &str,
    proven: &ProvenSet,
    tasks: &BTreeSet<String>,
    emit: &mut dyn FnMut(Family, Literal, &str),
) {
    for (i, entry) in body.iter().enumerate() {
        let target = &entry.lit;
        // Negated occurrences count as listeners (roles) but are satisfied
        // by absence — they contribute no demand; out-of-fragment targets
        // (arith args, temporal) contribute none either (REQ-401).
        if !entry.joinable
            || target.negation
            || is_var_name(target)
            || synthetic_name(target.name())
        {
            continue;
        }
        let fam = family(target, tasks);
        if matches!(fam, Family::Malformed(_)) {
            continue; // no well-formed instance to demand
        }
        let tvars = term_vars(target);
        if tvars.is_empty() && target.predicate_args().is_empty() {
            // Flat body/goal literal: syntactic demand, as before —
            // independent of what else the body carries.
            if !proven.contains(target) {
                emit(fam, target.clone(), listener);
            }
            continue;
        }
        let siblings: Vec<&BodyLit> = body
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, l)| l)
            .collect();
        if tvars.is_empty() {
            // Ground parameterised: demanded under the empty witness —
            // every sibling must itself be an in-fragment ground proven
            // fact (an unjoinable sibling cannot witness), and at least
            // one such binder must exist: a ground parameterised goal
            // with NO proven co-body binder is the REQ-401 out-of-fragment
            // single-body / goal-only case (surfacing it needs abduction),
            // so a vacuously-true empty sibling set emits no demand.
            let witnessed = !siblings.is_empty()
                && siblings
                    .iter()
                    .all(|s| s.joinable && is_ground(&s.lit) && proven.contains(&s.lit));
            if witnessed && !proven.contains(target) {
                emit(fam, target.clone(), listener);
            }
            continue;
        }
        // Variable template: in-fragment only when every template variable
        // is bound by the sibling join and every sibling is joinable.
        if siblings.iter().any(|s| !s.joinable || is_var_name(&s.lit)) {
            continue;
        }
        let mut svars: HashSet<spindle_core::intern::SymbolId> = HashSet::new();
        for s in &siblings {
            svars.extend(term_vars(&s.lit));
        }
        if !tvars.is_subset(&svars) {
            continue; // unbound / partially-bound — no ground demand
        }
        // Bound the join: after each sibling, project every binding onto
        // the variables still needed — the target's plus the remaining
        // siblings' — and deduplicate. Witness *existence* is all demand
        // needs, and retaining sibling-only bindings would materialise
        // their Cartesian product (n^k for k independent siblings, each
        // with n matching facts) before the final dedup, blowing the
        // NFR-401 budget on a small valid corpus.
        let mut live_after: Vec<HashSet<spindle_core::intern::SymbolId>> =
            Vec::with_capacity(siblings.len());
        {
            let mut live = tvars.clone();
            for s in siblings.iter().rev() {
                live_after.push(live.clone());
                live.extend(term_vars(&s.lit));
            }
            live_after.reverse();
        }
        let mut bindings: Vec<Substitution> = vec![Substitution::default()];
        for (k, s) in siblings.iter().enumerate() {
            let mut next: Vec<Substitution> = Vec::new();
            let mut kept: HashSet<Vec<(spindle_core::intern::SymbolId, spindle_core::term::Term)>> =
                HashSet::new();
            let mut keep = |b: &Substitution, next: &mut Vec<Substitution>| {
                let mut proj = Substitution::default();
                for (var, t) in &b.terms {
                    if live_after[k].contains(var) {
                        proj.terms.insert(*var, t.clone());
                    }
                }
                let mut key: Vec<_> = proj.terms.iter().map(|(v, t)| (*v, t.clone())).collect();
                key.sort_by_key(|&(v, _)| v);
                if kept.insert(key) {
                    next.push(proj);
                }
            };
            for b in &bindings {
                let sb = apply_substitution_to_literal(&s.lit, b);
                if is_ground(&sb) {
                    if proven.contains(&sb) {
                        keep(b, &mut next);
                    }
                    continue;
                }
                for g in proven.candidates(&sb) {
                    if let Some(m) = match_literal(&sb, g)
                        && let Some(merged) = merge_subst(b, &m)
                    {
                        keep(&merged, &mut next);
                    }
                }
            }
            bindings = next;
            if bindings.is_empty() {
                break;
            }
        }
        let mut seen: HashSet<(bool, String)> = HashSet::new();
        for b in &bindings {
            let inst = apply_substitution_to_literal(target, b);
            if is_ground(&inst) && !proven.contains(&inst) && seen.insert(lit_key(&inst)) {
                emit(fam.clone(), inst, listener);
            }
        }
    }
}

// ── Redefinition annotations (REQ-407 / OBS-402) ────────────────────────

/// A journal annotation: this Entry overwrote another signer's winning
/// value for a CON-401 key of a family's documentation.
#[derive(Clone, Debug, PartialEq)]
pub struct Redefinition {
    /// Sentence-id of the overwriting Entry.
    pub sid: String,
    pub family: Family,
    pub key: String,
    pub previous_writer: String,
}

/// Replay the admitted corpus in canonical order and surface every
/// cross-signer overwrite of a winning CON-401 key value (REQ-407:
/// visible, never prevented). Tombstoned, shadowed, and quarantined
/// Entries never count.
pub fn redefinitions(closure: &Closure) -> Vec<Redefinition> {
    let theory = &closure.theory;
    let mut winners: HashMap<(Family, String), String> = HashMap::new();
    let mut out = Vec::new();
    for a in &closure.admitted {
        if a.retracted || a.label_shadowed {
            continue;
        }
        let SpeechAct::Assert { spl, sentence_id } = &a.act else {
            continue;
        };
        // Any write of a CON-401 key counts toward redefinition (a malformed
        // overwrite still contests the family), so unlike the provenance
        // pass this ignores the `conforming` flag.
        let mut writes: Vec<(Family, String)> = doc_writes(theory, spl)
            .into_iter()
            .map(|w| (w.family, w.key))
            .collect();
        writes.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
        for (fam, key) in writes {
            if let Some(prev) = winners.get(&(fam.clone(), key.clone()))
                && prev != &a.entry.signer
            {
                out.push(Redefinition {
                    sid: sentence_id.clone(),
                    family: fam.clone(),
                    key: key.clone(),
                    previous_writer: prev.clone(),
                });
            }
            winners.insert((fam, key), a.entry.signer.clone());
        }
    }
    out
}

// ── Advisory (REQ-406) ──────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdvisoryKind {
    InertFamily,
    Sibling,
}

impl AdvisoryKind {
    pub fn name(self) -> &'static str {
        match self {
            AdvisoryKind::InertFamily => "inert-family",
            AdvisoryKind::Sibling => "sibling",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Candidate {
    pub literal: String,
    pub listener: String,
}

#[derive(Clone, Debug)]
pub struct Advisory {
    pub kind: AdvisoryKind,
    pub family: Family,
    pub candidates: Vec<Candidate>,
}

const CANDIDATE_CAP: usize = 5;

/// Compute the near-miss advisory for a single asserted fact against a
/// reference view (REQ-406). Returns None when no advisory applies.
pub fn advisory(view: &VocabView, fact: &Literal) -> Option<Advisory> {
    let fam = family(fact, &view.tasks);
    if fam
        .registry_functor()
        .as_deref()
        .and_then(builtin)
        .is_some()
    {
        return None; // built-ins never trigger the advisory (REQ-404)
    }
    let row = view.rows.iter().find(|r| r.family == fam);
    let roles = row.map(|r| &r.roles);
    let has_listener = roles
        .map(|r| r.contains(&Role::Body) || r.contains(&Role::Goal))
        .unwrap_or(false);

    let fact_args = resolved_args(&fam, fact);
    let shares = |d: &Demand| -> bool {
        let cargs = resolved_args(&d.family, &d.literal);
        cargs.iter().any(|c| fact_args.contains(c))
    };
    let sort_key = |d: &Demand| (display_literal(&d.literal), d.listener.clone());

    if !has_listener {
        // Inert-family advisory — unless documented as discovery.
        if row
            .map(|r| r.doc.kind.as_deref() == Some("discovery"))
            .unwrap_or(false)
        {
            return None;
        }
        let mut pool: Vec<&Demand> = view.demands.iter().collect();
        pool.sort_by_key(|d| (!shares(d), sort_key(d)));
        return Some(Advisory {
            kind: AdvisoryKind::InertFamily,
            family: fam,
            candidates: to_candidates(&pool),
        });
    }

    // Sibling advisory: the asserted literal is not itself a demanded
    // instance, while ≥ 1 same-family demanded-but-unproven sibling exists.
    let fkey = lit_key(fact);
    if view
        .demands
        .iter()
        .any(|d| d.family == fam && lit_key(&d.literal) == fkey)
    {
        return None;
    }
    let mut sibs: Vec<&Demand> = view.demands.iter().filter(|d| d.family == fam).collect();
    if sibs.is_empty() {
        return None;
    }
    // Triggering siblings ranked first and never dropped by the cap.
    sibs.sort_by_key(|d| sort_key(d));
    let mut rest: Vec<&Demand> = view.demands.iter().filter(|d| d.family != fam).collect();
    rest.sort_by_key(|d| (!shares(d), sort_key(d)));
    let mut pool = sibs;
    pool.extend(rest);
    Some(Advisory {
        kind: AdvisoryKind::Sibling,
        family: fam,
        candidates: to_candidates(&pool),
    })
}

fn to_candidates(pool: &[&Demand]) -> Vec<Candidate> {
    pool.iter()
        .take(CANDIDATE_CAP)
        .map(|d| Candidate {
            literal: display_literal(&d.literal),
            listener: d.listener.clone(),
        })
        .collect()
}

/// The fact's resolved argument(s) for candidate ranking (REQ-406): the
/// trailing suffix past the family stem for a flat atom, the ground terms
/// for a parameterised literal.
fn resolved_args(fam: &Family, lit: &Literal) -> Vec<String> {
    match fam {
        Family::Legacy(stem) => {
            let atom = lit.name();
            atom.strip_prefix(stem.as_str())
                .and_then(|r| r.strip_prefix('-'))
                .filter(|r| !r.is_empty())
                .map(|r| vec![r.to_string()])
                .unwrap_or_default()
        }
        Family::Predicate(_) => {
            // Read the structured argument terms directly: rendering to SPL
            // and splitting on whitespace mis-tokenises a quoted argument
            // (`(finding m1 "needs retry")`) and double-wraps a negated
            // literal (`(not (p x))`), corrupting the shares() ranking.
            lit.predicate_args().iter().map(|t| t.to_string()).collect()
        }
        Family::Malformed(_) => Vec::new(),
    }
}

// ── JSON (CON-403) ──────────────────────────────────────────────────────

impl VocabRow {
    pub fn to_json(&self) -> serde_json::Value {
        let mut o = serde_json::Map::new();
        o.insert("kind".into(), self.family.kind().into());
        o.insert("family".into(), self.family.rendered().into());
        if let Family::Predicate(sym) = &self.family
            && sym.arity() >= 1
        {
            // CON-403: functor/arity broken out only for predicate rows
            // with arity ≥ 1; a nullary doc-target row (always detached,
            // CON-402) carries the rendered family alone.
            o.insert("functor".into(), sym.functor().to_string().into());
            o.insert("arity".into(), sym.arity().into());
        }
        o.insert(
            "roles".into(),
            self.roles
                .iter()
                .map(|r| serde_json::Value::from(r.name()))
                .collect::<Vec<_>>()
                .into(),
        );
        o.insert("class".into(), self.class.name().into());
        o.insert("built_in".into(), self.built_in.into());
        let d = &self.doc;
        let mut doc = serde_json::Map::new();
        doc.insert("description".into(), d.description.clone().into());
        if let Some(k) = &d.kind {
            doc.insert("kind".into(), k.clone().into());
        }
        if let Some(a) = &d.asserter {
            doc.insert("asserter".into(), a.clone().into());
        }
        doc.insert("documenter".into(), d.documenter.clone().into());
        doc.insert("redefined".into(), d.redefined.into());
        doc.insert(
            "malformed".into(),
            d.malformed
                .iter()
                .map(|k| serde_json::Value::from(k.as_str()))
                .collect::<Vec<_>>()
                .into(),
        );
        doc.insert("detached".into(), d.detached.into());
        o.insert("doc".into(), doc.into());
        o.into()
    }
}

impl Advisory {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "kind": self.kind.name(),
            "family": self.family.rendered(),
            "family_kind": self.family.kind(),
            "candidates": self.candidates.iter().map(|c| serde_json::json!({
                "literal": c.literal,
                "listener": c.listener,
            })).collect::<Vec<_>>(),
        })
    }
}

/// Rows in CON-403 order: kind `predicate` < `legacy` < `malformed`, then
/// family byte-lexicographic. `view()` already emits this order (BTreeMap
/// over the Family Ord), pinned here for the JSON contract.
pub fn view_json(v: &VocabView) -> serde_json::Value {
    v.rows
        .iter()
        .map(VocabRow::to_json)
        .collect::<Vec<_>>()
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::envelope::{Entry, Hlc};

    fn key(seed: u8) -> ed25519_dalek::SigningKey {
        ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
    }

    fn node(seed: u8) -> u64 {
        did_crdt::core::validate::node_id_from_pubkey(key(seed).verifying_key().as_bytes())
    }

    fn did(seed: u8) -> String {
        format!("did:crdt:{}", format!("{seed:02x}").repeat(32))
    }

    struct Fixture {
        entries: Vec<Entry>,
        counter: u64,
    }

    impl Fixture {
        fn new() -> Fixture {
            Fixture {
                entries: Vec::new(),
                counter: 0,
            }
        }

        fn add(&mut self, seed: u8, act: SpeechAct) -> String {
            self.counter += 1;
            let hlc = Hlc {
                wall_ms: 1_752_000_000_000 + self.counter,
                logical: 0,
                node_id: node(seed),
            };
            let e = Entry::create(
                "th-x",
                hlc,
                &did(seed),
                &format!("{}#key-0", did(seed)),
                &act,
                "2026-07-11T00:00:00Z",
                &key(seed),
            );
            let sid = Entry::sentence_id("th-x", &did(seed), hlc);
            self.entries.push(e);
            sid
        }

        fn assert_spl(&mut self, seed: u8, spl: &str) -> String {
            let sid = self.next_sid(seed);
            self.add(
                seed,
                SpeechAct::Assert {
                    sentence_id: sid.clone(),
                    spl: spl.to_string(),
                },
            );
            sid
        }

        fn next_sid(&self, seed: u8) -> String {
            let hlc = Hlc {
                wall_ms: 1_752_000_000_000 + self.counter + 1,
                logical: 0,
                node_id: node(seed),
            };
            Entry::sentence_id("th-x", &did(seed), hlc)
        }

        fn close(&self) -> Closure {
            let resolve = |d: &str, _k: &str| -> Option<ed25519_dalek::VerifyingKey> {
                (1u8..=9)
                    .find(|s| did(*s) == d)
                    .map(|s| key(s).verifying_key())
            };
            crate::core::closure::close(&self.entries, "th-x", "genesis", &resolve, "", NOW)
                .unwrap()
        }
    }

    const NOW: i64 = 1_784_000_000_000;

    fn no_tasks() -> BTreeSet<String> {
        BTreeSet::new()
    }

    fn tasks(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    fn lit(text: &str) -> Literal {
        parse_one_literal(text).unwrap()
    }

    fn row<'a>(v: &'a VocabView, kind: &str, family: &str) -> &'a VocabRow {
        v.rows
            .iter()
            .find(|r| r.family.kind() == kind && r.family.rendered() == family)
            .unwrap_or_else(|| {
                panic!(
                    "no row ({kind}, {family}); have {:?}",
                    v.rows
                        .iter()
                        .map(|r| (r.family.kind(), r.family.rendered()))
                        .collect::<Vec<_>>()
                )
            })
    }

    // ── CON-402 family resolution (TEST-401 / TEST-404) ─────────────────

    #[test]
    fn parameterised_literal_families_on_predicate_symbol() {
        let f = family(&lit("(ci-green m1)"), &no_tasks());
        assert_eq!(f.kind(), "predicate");
        assert_eq!(f.rendered(), "ci-green/1");
        let f2 = family(&lit("(commitment-state c1 fulfilled)"), &no_tasks());
        assert_eq!(f2.rendered(), "commitment-state/2");
    }

    #[test]
    fn parameterised_family_immune_to_task_split() {
        // Rule 1 never consults tasks (ADR-402).
        let f = family(&lit("(ci-green m1)"), &tasks(&["green", "m1"]));
        assert_eq!(f.rendered(), "ci-green/1");
        assert_eq!(f.kind(), "predicate");
    }

    #[test]
    fn flat_atom_spelled_like_indicator_stays_legacy() {
        // `/` is an atom char: a flat `p/1` must never merge with the
        // parameterised family p/1 (CON-402 collision fix, 0.5.0 #1).
        let flat = family(&lit("p/1"), &no_tasks());
        assert_eq!(flat.kind(), "legacy");
        assert_eq!(flat.rendered(), "p/1");
        let param = family(&lit("(p x)"), &no_tasks());
        assert_eq!(param.kind(), "predicate");
        assert_eq!(param.rendered(), "p/1");
        assert_ne!(flat, param);
    }

    #[test]
    fn builtin_ground_patterns_resolve() {
        // TEST-404 positive rows.
        let t = no_tasks();
        for (atom, fam) in [
            ("claim-v3-m1", "claim"),
            ("unclaim-v1-x", "unclaim"),
            ("block-v2-t", "block"),
            ("unblock-v10-t", "unblock"),
            ("state-claimed-v1-x", "state-claimed"),
            ("state-unclaimed-v1-x", "state-unclaimed"),
            ("state-blocked-v1-x", "state-blocked"),
            ("state-unblocked-v1-x", "state-unblocked"),
            ("stale-v1-x", "stale"),
            ("timeout-v2-y", "timeout"),
            ("agent-alice-available", "agent-available"),
            ("no-deps-x", "no-deps"),
            ("task-m1", "task"),
            ("ready-m1", "ready"),
            ("completed-m1", "completed"),
            ("claimed-m1", "claimed"),
            ("blocked-m1", "blocked"),
            ("upstream-blocked-m1", "upstream-blocked"),
            ("decomposed-m1", "decomposed"),
            ("permanently-failed-m1", "permanently-failed"),
            ("assign-to-m1-alice", "assign-to"),
            ("failed-m1", "failed"),
            ("discovered-api-deprecated", "discovered"),
            ("decided-x", "decided"),
            ("blocked-by-vendor", "blocked-by"),
            ("requires-review", "requires"),
            ("verified-m1", "verified"),
            ("finding-x", "finding"),
            ("approach-x", "approach"),
            ("insight-x", "insight"),
            ("partial-x", "partial"),
        ] {
            assert_eq!(
                flat_family(atom, &t),
                Family::Legacy(fam.to_string()),
                "{atom}"
            );
        }
    }

    #[test]
    fn unlisted_versioned_atom_is_not_builtin() {
        // TEST-404 negative-output: `deploy-v3-m1` matches nothing in the
        // closed registry; with task m1 declared it strips the suffix.
        assert_eq!(
            flat_family("deploy-v3-m1", &no_tasks()),
            Family::Legacy("deploy-v3-m1".to_string())
        );
        assert_eq!(
            flat_family("deploy-v3-m1", &tasks(&["m1"])),
            Family::Legacy("deploy-v3".to_string())
        );
    }

    #[test]
    fn longest_declared_task_suffix_wins() {
        let f = flat_family("is-x-green", &tasks(&["green", "x-green"]));
        assert_eq!(f, Family::Legacy("is".to_string()));
    }

    #[test]
    fn adversarial_task_split_is_confined_to_flat_atoms() {
        // Declare task `green` → flat family `is-green` splits to `is`
        // (ADR-402 accepted hazard, legacy path only).
        assert_eq!(
            flat_family("is-green", &tasks(&["green"])),
            Family::Legacy("is".to_string())
        );
        assert_eq!(
            flat_family("is-green", &no_tasks()),
            Family::Legacy("is-green".to_string())
        );
    }

    #[test]
    fn arity_zero_never_takes_rule_one() {
        // A bare atom resolves through the legacy rules even though spindle
        // would happily form a PredicateSymbol at arity 0.
        let f = family(&lit("verified-m1"), &no_tasks());
        assert_eq!(f, Family::Legacy("verified".to_string()));
    }

    // ── Built-in registry (TEST-404) ────────────────────────────────────

    #[test]
    fn registry_is_functor_level_and_closed() {
        assert!(builtin("verified").is_some());
        assert!(builtin("commitment-state").is_some());
        assert!(builtin("failed").is_some());
        assert_eq!(builtin("failed").unwrap().kind, "control");
        assert!(builtin("deploy").is_none());
        assert_eq!(BUILTINS.len(), 32);
    }

    // ── View: roles, classes, docs (TEST-401/402) ───────────────────────

    #[test]
    fn witness_join_hole() {
        // (review-approved m1) proven binds ?t=m1; (ci-green m1) demanded
        // and unproven → ci-green/1 is a hole (TEST-401).
        let mut f = Fixture::new();
        f.assert_spl(
            1,
            "(normally r-verified (and (ci-green ?t) (review-approved ?t)) (verified ?t))",
        );
        f.assert_spl(1, "(given (review-approved m1))");
        let v = view(&f.close());
        let r = row(&v, "predicate", "ci-green/1");
        assert_eq!(r.class, Class::Hole);
        assert!(r.roles.contains(&Role::Body));
        assert!(
            v.demands
                .iter()
                .any(|d| display_literal(&d.literal) == "(ci-green m1)"
                    && d.listener == "r-verified"),
            "demanded instance must be the exact ground literal: {:?}",
            v.demands
                .iter()
                .map(|d| display_literal(&d.literal))
                .collect::<Vec<_>>()
        );
        // review-approved/1 has no proven co-body witness for its own
        // template (ci-green is unproven) → no demand → active.
        let ra = row(&v, "predicate", "review-approved/1");
        assert_eq!(ra.class, Class::Active);
    }

    #[test]
    fn witnessless_template_is_active_not_hole() {
        // Single-body template with an unbound variable: out of fragment,
        // deliberately under-reported (TEST-401 negative-output).
        let mut f = Fixture::new();
        f.assert_spl(1, "(normally r (ci-green ?t) (verified ?t))");
        let v = view(&f.close());
        let r = row(&v, "predicate", "ci-green/1");
        assert_eq!(r.class, Class::Active);
        assert!(v.demands.is_empty());
    }

    #[test]
    fn multi_variable_witness_join() {
        // (and (ci-green ?t) (assigned ?t ?a) (review-approved ?t)) demands
        // (ci-green c) for each (c, a) with both siblings proven.
        let mut f = Fixture::new();
        f.assert_spl(
            1,
            "(normally r (and (ci-green ?t) (assigned ?t ?a) (review-approved ?t)) (verified ?t))",
        );
        f.assert_spl(1, "(given (assigned m1 alice))");
        f.assert_spl(1, "(given (review-approved m1))");
        f.assert_spl(1, "(given (assigned m2 bob))"); // m2 lacks review
        let v = view(&f.close());
        let demanded: Vec<String> = v
            .demands
            .iter()
            .filter(|d| d.family.rendered() == "ci-green/1")
            .map(|d| display_literal(&d.literal))
            .collect();
        assert_eq!(demanded, vec!["(ci-green m1)".to_string()]);
    }

    #[test]
    fn flat_body_atom_demand_is_syntactic() {
        let mut f = Fixture::new();
        f.assert_spl(1, "(normally r (and ci-green-m1 review-ok-m1) verified-m1)");
        let v = view(&f.close());
        // Both flat body atoms demanded unconditionally.
        let lits: Vec<String> = v
            .demands
            .iter()
            .map(|d| display_literal(&d.literal))
            .collect();
        assert!(lits.contains(&"ci-green-m1".to_string()));
        assert!(lits.contains(&"review-ok-m1".to_string()));
        let r = row(&v, "legacy", "ci-green-m1");
        assert_eq!(r.class, Class::Hole);
    }

    #[test]
    fn mixed_corpus_shows_two_verified_rows_both_builtin() {
        let mut f = Fixture::new();
        f.assert_spl(1, "(given verified-m1)");
        f.assert_spl(1, "(given (verified m1))");
        let v = view(&f.close());
        let legacy = row(&v, "legacy", "verified");
        let param = row(&v, "predicate", "verified/1");
        assert!(legacy.built_in && param.built_in);
        assert_eq!(
            legacy.doc.description.as_deref(),
            Some("evidence a task's work is verified (RECOMMENDED evidence-rule conclusion)")
        );
        assert!(legacy.doc.documenter.is_none());
        assert!(!legacy.doc.redefined);
        // Built-in facts are never orphans.
        assert_eq!(legacy.class, Class::Active);
        assert_eq!(param.class, Class::Active);
    }

    #[test]
    fn collision_rows_never_merge() {
        let mut f = Fixture::new();
        f.assert_spl(1, "(given (p x))");
        f.assert_spl(1, "(given p/1)");
        let v = view(&f.close());
        let kinds: Vec<(&str, String)> = v
            .rows
            .iter()
            .filter(|r| r.family.rendered() == "p/1")
            .map(|r| (r.family.kind(), r.family.rendered()))
            .collect();
        assert_eq!(kinds.len(), 2, "predicate p/1 and legacy p/1 distinct");
        // Ordering: predicate before legacy (CON-403).
        assert_eq!(kinds[0].0, "predicate");
        assert_eq!(kinds[1].0, "legacy");
    }

    #[test]
    fn orphan_and_discovery_doc() {
        let mut f = Fixture::new();
        f.assert_spl(1, "(given deploy-thing)");
        let v = view(&f.close());
        assert_eq!(row(&v, "legacy", "deploy-thing").class, Class::Orphan);

        // Discovery-kind documentation lifts the orphan class (REQ-401).
        let mut f = Fixture::new();
        f.assert_spl(1, "(given deploy-thing)");
        f.assert_spl(
            1,
            "(meta deploy-thing (description \"ad-hoc deploy marker\") (kind discovery))",
        );
        let v = view(&f.close());
        assert_eq!(row(&v, "legacy", "deploy-thing").class, Class::Active);
    }

    #[test]
    fn builtin_fact_never_orphan() {
        let mut f = Fixture::new();
        f.assert_spl(1, "(given completed-x)");
        let v = view(&f.close());
        assert_eq!(row(&v, "legacy", "completed").class, Class::Active);
    }

    #[test]
    fn predicate_meta_target_documents_symbol() {
        // TEST-402: standalone meta-target and inline declaration both key
        // documentation on the predicate symbol.
        let mut f = Fixture::new();
        f.assert_spl(1, "(given (ci-green m1))");
        f.assert_spl(
            1,
            "(meta (predicate ci-green 1) (description \"CI pipeline green for task ?t\") (kind evidence) (asserter \"role:ci\"))",
        );
        let v = view(&f.close());
        let r = row(&v, "predicate", "ci-green/1");
        assert_eq!(
            r.doc.description.as_deref(),
            Some("CI pipeline green for task ?t")
        );
        assert_eq!(r.doc.kind.as_deref(), Some("evidence"));
        assert_eq!(r.doc.asserter.as_deref(), Some("role:ci"));
        assert_eq!(r.doc.documenter.as_deref(), Some(did(1).as_str()));
        assert!(!r.doc.redefined);
        assert!(!r.doc.detached);
    }

    #[test]
    fn inline_declaration_documents_symbol() {
        let mut f = Fixture::new();
        f.assert_spl(
            1,
            "(predicate ci-green ((task symbol)) (description \"CI green\") (kind evidence))",
        );
        f.assert_spl(1, "(given (ci-green m1))");
        let v = view(&f.close());
        let r = row(&v, "predicate", "ci-green/1");
        assert_eq!(r.doc.description.as_deref(), Some("CI green"));
        assert!(!r.doc.detached);
    }

    #[test]
    fn rule_label_shadow_closed_for_predicates_open_for_legacy() {
        // A rule *labelled* ci-green cannot shadow predicate-target docs
        // (distinct namespaces); on the legacy label carrier the rule wins.
        let mut f = Fixture::new();
        f.assert_spl(1, "(normally ci-green qa-ok deploy-ok)");
        f.assert_spl(
            1,
            "(meta (predicate ci-green 1) (description \"symbol doc\"))",
        );
        f.assert_spl(1, "(given (ci-green m1))");
        f.assert_spl(1, "(meta ci-green (description \"label doc\"))");
        let v = view(&f.close());
        let r = row(&v, "predicate", "ci-green/1");
        assert_eq!(r.doc.description.as_deref(), Some("symbol doc"));
        // No legacy `ci-green` doc row: the label names an admitted rule.
        assert!(!v.rows.iter().any(|r| r.family.kind() == "legacy"
            && r.family.rendered() == "ci-green"
            && r.doc.is_documented()));
    }

    #[test]
    fn one_malformed_key_does_not_undocument_family() {
        // Per-key evaluation (REQ-402): bogus kind is surfaced, the
        // conforming description stands.
        let mut f = Fixture::new();
        f.assert_spl(1, "(given deploy-thing)");
        f.assert_spl(
            1,
            "(meta deploy-thing (description \"a deploy marker\") (kind bogus))",
        );
        let v = view(&f.close());
        let r = row(&v, "legacy", "deploy-thing");
        assert_eq!(r.doc.description.as_deref(), Some("a deploy marker"));
        assert_eq!(r.doc.malformed, vec!["kind".to_string()]);
        assert!(r.doc.kind.is_none());
    }

    #[test]
    fn oversized_or_control_descriptions_are_malformed() {
        let mut f = Fixture::new();
        f.assert_spl(1, "(given deploy-thing)");
        let big = "x".repeat(513);
        f.assert_spl(1, &format!("(meta deploy-thing (description \"{big}\"))"));
        let v = view(&f.close());
        let r = row(&v, "legacy", "deploy-thing");
        assert!(r.doc.description.is_none());
        assert_eq!(r.doc.malformed, vec!["description".to_string()]);
        assert_eq!(r.class, Class::Orphan, "malformed doc does not lift orphan");
    }

    #[test]
    fn tombstoned_doc_reverts_to_prior_winner() {
        // TEST-402: retracting the winning Entry revives the previous write.
        let mut f = Fixture::new();
        f.assert_spl(1, "(given deploy-thing)");
        f.assert_spl(1, "(meta deploy-thing (description \"first\"))");
        let sid2 = f.assert_spl(2, "(meta deploy-thing (description \"second\"))");
        let v = view(&f.close());
        assert_eq!(
            row(&v, "legacy", "deploy-thing").doc.description.as_deref(),
            Some("second")
        );
        f.add(
            2,
            SpeechAct::Retract {
                target: sid2,
                reason: "withdrawn".into(),
            },
        );
        let v = view(&f.close());
        let r = row(&v, "legacy", "deploy-thing");
        assert_eq!(r.doc.description.as_deref(), Some("first"));
        // Retract-and-redefine by one signer never flags (REQ-407) — and
        // the tombstoned second signer no longer counts.
        assert!(!r.doc.redefined);
        assert_eq!(r.doc.documenter.as_deref(), Some(did(1).as_str()));
    }

    #[test]
    fn redefinition_provenance() {
        // TEST-407: second distinct signer → redefined, documenter = last
        // writer in canonical order.
        let mut f = Fixture::new();
        f.assert_spl(1, "(given deploy-thing)");
        f.assert_spl(1, "(meta deploy-thing (description \"mine\"))");
        f.assert_spl(2, "(meta deploy-thing (description \"theirs\"))");
        let v = view(&f.close());
        let r = row(&v, "legacy", "deploy-thing");
        assert_eq!(r.doc.description.as_deref(), Some("theirs"));
        assert_eq!(r.doc.documenter.as_deref(), Some(did(2).as_str()));
        assert!(r.doc.redefined);

        // Same-signer update is not a redefinition.
        let mut f = Fixture::new();
        f.assert_spl(1, "(given deploy-thing)");
        f.assert_spl(1, "(meta deploy-thing (description \"v1\"))");
        f.assert_spl(1, "(meta deploy-thing (description \"v2\"))");
        let v = view(&f.close());
        let r = row(&v, "legacy", "deploy-thing");
        assert_eq!(r.doc.description.as_deref(), Some("v2"));
        assert!(!r.doc.redefined);
    }

    #[test]
    fn redefinition_log_annotation() {
        // TEST-407: a cross-signer overwrite of a winning CON-401 key is
        // annotated with family, key, and previous writer; same-signer
        // updates and retract-then-redefine never are.
        let mut f = Fixture::new();
        f.assert_spl(1, "(meta deploy-thing (description \"mine\"))");
        f.assert_spl(1, "(meta deploy-thing (description \"mine v2\"))");
        let sid3 = f.assert_spl(
            2,
            "(meta deploy-thing (description \"theirs\") (kind state))",
        );
        let r = redefinitions(&f.close());
        assert_eq!(r.len(), 1, "only the cross-signer description overwrite");
        assert_eq!(r[0].sid, sid3);
        assert_eq!(r[0].family, Family::Legacy("deploy-thing".into()));
        assert_eq!(r[0].key, "description");
        assert_eq!(r[0].previous_writer, did(1));

        // Retract-and-redefine by one signer does not flag; a tombstoned
        // Entry never counts as the previous writer.
        let mut f = Fixture::new();
        let s1 = f.assert_spl(1, "(meta deploy-thing (description \"v1\"))");
        f.add(
            1,
            SpeechAct::Retract {
                target: s1,
                reason: "withdrawn".into(),
            },
        );
        f.assert_spl(1, "(meta deploy-thing (description \"v2\"))");
        assert!(redefinitions(&f.close()).is_empty());

        // Predicate-target carrier is annotated on its symbol too.
        let mut f = Fixture::new();
        f.assert_spl(1, "(meta (predicate ci-green 1) (description \"a\"))");
        let sid = f.assert_spl(2, "(meta (predicate ci-green 1) (description \"b\"))");
        let r = redefinitions(&f.close());
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].sid, sid);
        assert_eq!(r[0].family.rendered(), "ci-green/1");
    }

    #[test]
    fn detached_documentation_is_surfaced() {
        // A documented family joining no occurrence (REQ-401): a label no
        // occurrence resolves to, and a nullary predicate target.
        let mut f = Fixture::new();
        f.assert_spl(1, "(meta ghost-family (description \"documents nothing\"))");
        f.assert_spl(
            1,
            "(meta (predicate foo 0) (description \"nullary target\"))",
        );
        f.assert_spl(1, "(given foo)"); // resolves to Legacy(foo), not foo/0
        let v = view(&f.close());
        assert!(row(&v, "legacy", "ghost-family").doc.detached);
        assert!(row(&v, "predicate", "foo/0").doc.detached);
        // The flat occurrence row itself is not detached.
        assert!(!row(&v, "legacy", "foo").doc.detached);
    }

    #[test]
    fn corpus_meta_on_builtin_is_ignored() {
        // REQ-404 freeze: wire docs on a built-in functor (any carrier,
        // any arity) never override the fixed description.
        let mut f = Fixture::new();
        f.assert_spl(1, "(given verified-m1)");
        f.assert_spl(1, "(meta verified (description \"hijacked\"))");
        f.assert_spl(1, "(given (verified m1))");
        f.assert_spl(
            1,
            "(meta (predicate verified 1) (description \"hijacked too\"))",
        );
        let v = view(&f.close());
        assert_eq!(
            row(&v, "legacy", "verified").doc.description.as_deref(),
            Some("evidence a task's work is verified (RECOMMENDED evidence-rule conclusion)")
        );
        assert_eq!(
            row(&v, "predicate", "verified/1")
                .doc
                .description
                .as_deref(),
            Some("evidence a task's work is verified (RECOMMENDED evidence-rule conclusion)")
        );
    }

    #[test]
    fn goal_role_from_commitment_and_request() {
        let mut f = Fixture::new();
        f.add(
            1,
            SpeechAct::Commit {
                sentence_id: f.next_sid(1),
                trigger: "qa-signed".into(),
                by: None,
                goal: "legal-signed".into(),
            },
        );
        let v = view(&f.close());
        assert!(row(&v, "legacy", "qa-signed").roles.contains(&Role::Goal));
        assert!(
            row(&v, "legacy", "legal-signed")
                .roles
                .contains(&Role::Goal)
        );
        // Flat ground goal/trigger literals are demanded (hole class).
        assert_eq!(row(&v, "legacy", "legal-signed").class, Class::Hole);
    }

    #[test]
    fn negated_body_occurrence_counts_as_listener_but_not_demand() {
        let mut f = Fixture::new();
        f.assert_spl(1, "(normally r (not vetoed-x) approved-x)");
        let v = view(&f.close());
        let r = row(&v, "legacy", "vetoed-x");
        assert!(r.roles.contains(&Role::Body));
        assert!(
            !v.demands.iter().any(|d| d.family.rendered() == "vetoed-x"),
            "negated occurrences are satisfied by absence, never demanded"
        );
        assert_eq!(r.class, Class::Active);
    }

    #[test]
    fn head_of_non_fact_rule_is_never_hole() {
        let mut f = Fixture::new();
        f.assert_spl(1, "(normally r1 a-x b-x)");
        f.assert_spl(1, "(normally r2 b-x c-x)");
        let v = view(&f.close());
        // b-x heads r2 and is body of... wait: b-x is head of r1 and body of r2.
        let r = row(&v, "legacy", "b-x");
        assert!(r.roles.contains(&Role::Head) && r.roles.contains(&Role::Body));
        assert_ne!(r.class, Class::Hole);
    }

    // ── Advisory (REQ-406, pure part) ───────────────────────────────────

    #[test]
    fn inert_family_advisory() {
        let mut f = Fixture::new();
        f.assert_spl(1, "(normally r ci-green-m1 verified-m1)");
        let v = view(&f.close());
        let a = advisory(&v, &lit("random-note")).expect("inert advisory");
        assert_eq!(a.kind, AdvisoryKind::InertFamily);
        assert!(
            a.candidates
                .iter()
                .any(|c| c.literal == "ci-green-m1" && c.listener == "r")
        );
    }

    #[test]
    fn sibling_advisory_flat() {
        // TEST-406 canonical wrong-suffix case: ci-green-m2 asserted while
        // only ci-green-m1 is listened for. Both atoms share the family
        // `ci-green` via the declared-task suffix rules (CON-402 rule 3).
        let mut f = Fixture::new();
        f.assert_spl(1, "(given task-m1)");
        f.assert_spl(1, "(given task-m2)");
        f.assert_spl(1, "(normally r ci-green-m1 verified-m1)");
        f.assert_spl(1, "(given ci-green-m2)");
        let v = view(&f.close());
        let a = advisory(&v, &lit("ci-green-m2")).expect("sibling advisory");
        assert_eq!(a.kind, AdvisoryKind::Sibling);
        assert_eq!(a.candidates[0].literal, "ci-green-m1");
        assert_eq!(a.candidates[0].listener, "r");
    }

    #[test]
    fn sibling_advisory_parameterised_witness() {
        // (review-approved m1) proven binds ?t=m1 → (ci-green m1) demanded;
        // asserting (ci-green m2) is the near-miss.
        let mut f = Fixture::new();
        f.assert_spl(
            1,
            "(normally r-verified (and (ci-green ?t) (review-approved ?t)) (verified ?t))",
        );
        f.assert_spl(1, "(given (review-approved m1))");
        let v = view(&f.close());
        let a = advisory(&v, &lit("(ci-green m2)")).expect("sibling advisory");
        assert_eq!(a.kind, AdvisoryKind::Sibling);
        assert_eq!(a.candidates[0].literal, "(ci-green m1)");
        assert_eq!(a.candidates[0].listener, "r-verified");
    }

    #[test]
    fn no_sibling_advisory_without_live_demand() {
        // Template with no proven co-body witness: no live demand, no
        // advisory (TEST-406 negative-output).
        let mut f = Fixture::new();
        f.assert_spl(
            1,
            "(normally r-verified (and (ci-green ?t) (review-approved ?t)) (verified ?t))",
        );
        let v = view(&f.close());
        assert!(advisory(&v, &lit("(ci-green m2)")).is_none());
    }

    #[test]
    fn demanded_assert_gets_no_advisory() {
        let mut f = Fixture::new();
        f.assert_spl(1, "(normally r ci-green-m1 verified-m1)");
        let v = view(&f.close());
        assert!(advisory(&v, &lit("ci-green-m1")).is_none());
    }

    #[test]
    fn builtin_and_discovery_families_no_advisory() {
        let mut f = Fixture::new();
        f.assert_spl(1, "(normally r ci-green-m1 verified-m1)");
        f.assert_spl(
            1,
            "(meta scratch (description \"scratch notes\") (kind discovery))",
        );
        f.assert_spl(1, "(given scratch-a)");
        let v = view(&f.close());
        // Built-in functor (discovery vocabulary): never an advisory.
        assert!(advisory(&v, &lit("discovered-api-flaky")).is_none());
        // Corpus-documented kind discovery on the asserted family: no
        // inert advisory. (`scratch-a` resolves to Legacy("scratch-a") —
        // document the exact family the fact resolves to.)
        let mut f2 = Fixture::new();
        f2.assert_spl(
            1,
            "(meta scratch-a (description \"notes\") (kind discovery))",
        );
        let v2 = view(&f2.close());
        assert!(advisory(&v2, &lit("scratch-a")).is_none());
    }

    #[test]
    fn negatively_consumed_family_is_not_inert() {
        let mut f = Fixture::new();
        f.assert_spl(1, "(normally r (not vetoed-x) approved-x)");
        let v = view(&f.close());
        // vetoed-x has role body (negated counts): not inert. And it has
        // no unproven demanded sibling → no advisory at all.
        assert!(advisory(&v, &lit("vetoed-x")).is_none());
    }

    #[test]
    fn triggering_sibling_survives_the_cap() {
        // Five unrelated argument-sharing holes must not evict the
        // same-family sibling (REQ-406 ranking).
        let mut f = Fixture::new();
        f.assert_spl(1, "(given task-m1)");
        f.assert_spl(1, "(given task-m2)");
        f.assert_spl(
            1,
            "(normally r (and dep-a-m2 dep-b-m2 dep-c-m2 dep-d-m2 dep-e-m2 ci-green-m1) verified-m1)",
        );
        let v = view(&f.close());
        let a = advisory(&v, &lit("ci-green-m2")).expect("sibling advisory");
        assert_eq!(a.kind, AdvisoryKind::Sibling);
        assert_eq!(a.candidates.len(), 5);
        assert_eq!(
            a.candidates[0].literal, "ci-green-m1",
            "triggering sibling ranked first, never dropped"
        );
    }

    #[test]
    fn commitment_goal_assert_gets_no_advisory() {
        // The canonical fulfilment act: goal role is a listener.
        let mut f = Fixture::new();
        f.add(
            1,
            SpeechAct::Commit {
                sentence_id: f.next_sid(1),
                trigger: "".into(),
                by: None,
                goal: "legal-signed".into(),
            },
        );
        let v = view(&f.close());
        assert!(advisory(&v, &lit("legal-signed")).is_none());
    }

    // ── Determinism kernel (TEST-408 unit level) ────────────────────────

    #[test]
    fn view_json_deterministic_under_corpus_shuffle() {
        let mut f = Fixture::new();
        f.assert_spl(
            1,
            "(normally r-verified (and (ci-green ?t) (review-approved ?t)) (verified ?t))",
        );
        f.assert_spl(2, "(given (review-approved m1))");
        f.assert_spl(
            1,
            "(meta (predicate ci-green 1) (description \"CI green\"))",
        );
        f.assert_spl(2, "(given deploy-thing)");
        f.assert_spl(1, "(given task-m1)");
        f.assert_spl(2, "(given stale-note-m1)");
        let base = serde_json::to_string(&view_json(&view(&f.close()))).unwrap();

        let mut rev = Fixture::new();
        rev.entries = f.entries.iter().rev().cloned().collect();
        let again = serde_json::to_string(&view_json(&view(&rev.close()))).unwrap();
        assert_eq!(base, again);
    }

    #[test]
    fn tasks_derive_from_provable_task_facts() {
        let mut f = Fixture::new();
        f.assert_spl(1, "(given task-m1)");
        f.assert_spl(1, "(given stale-note-m1)");
        let v = view(&f.close());
        assert!(v.tasks.contains("m1"));
        // stale-note-m1 splits on the declared task m1.
        assert!(
            v.rows
                .iter()
                .any(|r| r.family == Family::Legacy("stale-note".to_string()))
        );
    }

    #[test]
    fn malformed_functor_is_its_own_row_never_a_drop() {
        // Construct a literal with a control-character functor
        // programmatically (the parser would reject it) and check family().
        use spindle_core::mode::Mode;
        use spindle_core::temporal::Temporal;
        let l = Literal::new(
            "bad\u{1}name",
            false,
            Mode::default(),
            Temporal::default(),
            vec!["x".to_string()],
        );
        let f = family(&l, &no_tasks());
        match &f {
            Family::Malformed(raw) => assert_eq!(raw, "bad\u{1}name"),
            other => panic!("expected malformed, got {other:?}"),
        }
        // Distinct malformed functors stay distinct.
        let l2 = Literal::new(
            "bad\u{2}name",
            false,
            Mode::default(),
            Temporal::default(),
            vec!["x".to_string()],
        );
        assert_ne!(f, family(&l2, &no_tasks()));
        // Escaped on display.
        assert_eq!(f.rendered(), "bad\\u{0001}name");
    }

    #[test]
    fn arity_zero_malformed_functor_families_malformed() {
        // An arity-0 literal with an empty or control-character functor must
        // family as Malformed, matching spindle's `Vocabulary::derive`
        // diagnostic keying — else the demand/advisory path keys Legacy while
        // the row is Malformed and they never join.
        use spindle_core::mode::Mode;
        use spindle_core::temporal::Temporal;
        let empty = Literal::new("", false, Mode::default(), Temporal::default(), vec![]);
        assert_eq!(
            family(&empty, &no_tasks()),
            Family::Malformed("".to_string())
        );
        let ctrl = Literal::new(
            "ba\u{1}d",
            false,
            Mode::default(),
            Temporal::default(),
            vec![],
        );
        assert_eq!(
            family(&ctrl, &no_tasks()),
            Family::Malformed("ba\u{1}d".to_string())
        );
        // A well-formed flat atom is unaffected (still Legacy). Use a
        // non-built-in stem so rule 2's ground-pattern table does not claim
        // it (`verified-m1` would family to the built-in `verified`).
        assert_eq!(
            family(&lit("stale-note-x"), &no_tasks()),
            Family::Legacy("stale-note-x".to_string())
        );
    }

    #[test]
    fn resolved_args_reads_structured_terms() {
        // A quoted multi-word argument stays one token, and a negated literal
        // does not fragment — the to_spl-split it replaced mis-tokenised both.
        let quoted = lit(r#"(finding m1 "needs retry")"#);
        let fam = family(&quoted, &no_tasks());
        assert_eq!(
            resolved_args(&fam, &quoted),
            vec!["m1".to_string(), "needs retry".to_string()]
        );
        let negated = lit("(not (ci-green m1))");
        let nfam = family(&negated, &no_tasks());
        assert_eq!(resolved_args(&nfam, &negated), vec!["m1".to_string()]);
    }

    #[test]
    fn proven_set_uses_accepted_weighted_conclusions() {
        let mut f = Fixture::new();
        f.assert_spl(1, "(given qa-signed)");
        f.assert_spl(1, "(normally r qa-signed release-ready)");
        let c = f.close();
        let p = ProvenSet::from_closure(&c);
        assert!(p.contains(&lit("qa-signed")), "given fact is proven");
        assert!(p.contains(&lit("release-ready")), "derived +d is proven");
        assert!(!p.contains(&lit("legal-signed")), "absent is not proven");
    }

    #[test]
    fn whitespace_variant_meta_is_not_invisible() {
        // Adversarial-review finding #1: `( meta …)` parses identically to
        // `(meta …)`; provenance and the journal annotation must see it.
        let mut f = Fixture::new();
        f.assert_spl(1, "(given deploy-thing)");
        f.assert_spl(1, "(meta deploy-thing (description \"mine\"))");
        let sid = f.assert_spl(2, "( meta deploy-thing (description \"hijack\"))");
        let c = f.close();
        let v = view(&c);
        let r = row(&v, "legacy", "deploy-thing");
        assert_eq!(r.doc.description.as_deref(), Some("hijack"));
        assert_eq!(
            r.doc.documenter.as_deref(),
            Some(did(2).as_str()),
            "whitespace variant must still attribute the documenter"
        );
        assert!(r.doc.redefined, "cross-signer overwrite must flag");
        let redefs = redefinitions(&c);
        assert!(
            redefs.iter().any(|x| x.sid == sid),
            "journal annotation must see the whitespace variant"
        );
    }

    #[test]
    fn escaped_keyword_meta_is_not_invisible() {
        // Reviewer finding: SPL quoted atoms unescape `\X` → `X` before
        // keyword dispatch, so ("me\ta" …) IS a meta form whose source
        // carries neither "meta" nor "predicate" as a byte substring.
        // Provenance and the journal annotation must still see it.
        let mut f = Fixture::new();
        f.assert_spl(1, "(given deploy-thing)");
        f.assert_spl(1, "(meta deploy-thing (description \"mine\"))");
        let sid = f.assert_spl(2, r#"("me\ta" deploy-thing (description "hijack"))"#);
        let c = f.close();
        let v = view(&c);
        let r = row(&v, "legacy", "deploy-thing");
        assert_eq!(r.doc.description.as_deref(), Some("hijack"));
        assert_eq!(
            r.doc.documenter.as_deref(),
            Some(did(2).as_str()),
            "escaped keyword must still attribute the documenter"
        );
        assert!(r.doc.redefined, "cross-signer overwrite must flag");
        let redefs = redefinitions(&c);
        assert!(
            redefs.iter().any(|x| x.sid == sid),
            "journal annotation must see the escaped keyword"
        );
    }

    #[test]
    fn derived_task_head_never_splits_families() {
        // CON-402: tasks are declared by admitted `(given task-X)` facts —
        // never by rule heads. A rule deriving task-green must not make
        // `is-green` resolve to family `is`.
        let mut f = Fixture::new();
        f.assert_spl(1, "(given go)");
        f.assert_spl(1, "(normally r go task-green)");
        f.assert_spl(1, "(given is-green)");
        f.assert_spl(1, "(given task-m1)");
        f.assert_spl(1, "(given stale-note-m1)");
        let v = view(&f.close());
        assert!(
            !v.tasks.contains("green"),
            "derived task-green is not a declaration: {:?}",
            v.tasks
        );
        assert!(v.tasks.contains("m1"), "declared task-m1 still counts");
        assert!(
            v.rows
                .iter()
                .any(|r| r.family == Family::Legacy("is-green".to_string())),
            "is-green must stay its own family"
        );
        assert!(
            v.rows
                .iter()
                .any(|r| r.family == Family::Legacy("stale-note".to_string())),
            "declared task m1 still splits stale-note-m1"
        );
    }

    #[test]
    fn ground_parameterised_goal_without_binder_is_out_of_fragment() {
        // REQ-401 out-of-fragment: a ground parameterised goal with no
        // proven co-body binder (the single-body / goal-only case) has no
        // demand — an empty sibling set must not witness vacuously.
        let mut f = Fixture::new();
        f.assert_spl(1, "(normally r1 (qa-ok m1) done-x)");
        let v = view(&f.close());
        assert!(
            !v.demands
                .iter()
                .any(|d| display_literal(&d.literal) == "(qa-ok m1)"),
            "binder-less ground parameterised goal must contribute no demand"
        );
        assert_eq!(row(&v, "predicate", "qa-ok/1").class, Class::Active);

        // With a proven flat co-body binder it re-enters the fragment.
        let mut f2 = Fixture::new();
        f2.assert_spl(1, "(given qa-gate)");
        f2.assert_spl(1, "(normally r1 (and qa-gate (qa-ok m1)) done-x)");
        let v2 = view(&f2.close());
        assert!(
            v2.demands
                .iter()
                .any(|d| display_literal(&d.literal) == "(qa-ok m1)"),
            "proven co-body binder restores the demand"
        );
        assert_eq!(row(&v2, "predicate", "qa-ok/1").class, Class::Hole);
    }

    #[test]
    fn admitted_double_underscore_vocabulary_stays_visible() {
        // REQ-401: the admission grammar does not reserve `__*` — only the
        // closure's own `__ct-`/`__trig-` constructs are synthetic. An
        // admitted user fact `(given __user)` must appear in the view.
        let mut f = Fixture::new();
        f.assert_spl(1, "(given __user)");
        let v = view(&f.close());
        assert!(
            v.rows
                .iter()
                .any(|r| r.family == Family::Legacy("__user".to_string())),
            "admitted __user family must be visible: {:?}",
            v.rows
                .iter()
                .map(|r| r.family.rendered())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn witness_join_dedups_sibling_only_bindings() {
        // The join projects bindings onto the variables the target and
        // remaining siblings need; sibling-only bindings (?a below) must
        // neither multiply the demanded set nor blow up intermediates.
        let mut f = Fixture::new();
        f.assert_spl(1, "(given (assigned m1 a1))");
        f.assert_spl(1, "(given (assigned m1 a2))");
        f.assert_spl(1, "(given (assigned m1 a3))");
        f.assert_spl(1, "(given (review-approved m1))");
        f.assert_spl(
            1,
            "(normally r (and (ci-green ?t) (assigned ?t ?a) (review-approved ?t)) (verified2 ?t))",
        );
        let v = view(&f.close());
        let demanded: Vec<String> = v
            .demands
            .iter()
            .filter(|d| d.family.rendered() == "ci-green/1")
            .map(|d| display_literal(&d.literal))
            .collect();
        assert_eq!(
            demanded,
            vec!["(ci-green m1)".to_string()],
            "one demanded instance regardless of how many ?a witnesses"
        );
    }

    #[test]
    fn flat_demand_survives_out_of_fragment_sibling() {
        // Adversarial-review finding #2: a flat atom's demand is syntactic
        // and must not be dropped because a sibling carries arithmetic.
        let mut f = Fixture::new();
        f.assert_spl(1, "(given task-m1)");
        f.assert_spl(1, "(normally r2 (and dep2-m1 (p (+ ?x 1))) goal2-m1)");
        let v = view(&f.close());
        assert!(
            v.demands
                .iter()
                .any(|d| display_literal(&d.literal) == "dep2-m1"),
            "flat sibling demand must survive: {:?}",
            v.demands
                .iter()
                .map(|d| display_literal(&d.literal))
                .collect::<Vec<_>>()
        );
        assert_eq!(row(&v, "legacy", "dep2").class, Class::Hole);
    }

    #[test]
    fn malformed_ord_uses_raw_functor() {
        // Adversarial-review finding #6: escaping is not injective; two
        // distinct raw functors whose escaped forms collide must stay
        // distinct families under Ord (else the view BTreeMap merges them).
        use spindle_core::mode::Mode;
        use spindle_core::temporal::Temporal;
        let raw = Literal::new(
            "a\u{1}b",
            false,
            Mode::default(),
            Temporal::default(),
            vec!["x".into()],
        );
        let spelled = Literal::new(
            "a\\u{0001}b",
            false,
            Mode::default(),
            Temporal::default(),
            vec!["x".into()],
        );
        let f1 = family(&raw, &no_tasks());
        let f2 = family(&spelled, &no_tasks());
        // `a\u{0001}b` spelled out is a LEGAL flat functor → Legacy, so
        // kinds already differ; the Ord check matters for two Malformed
        // raws that escape to the same bytes.
        let m1 = Family::Malformed("a\u{1}b".into());
        let m2 = Family::Malformed("a\\u{0001}b".into());
        assert_eq!(m1.rendered(), m2.rendered(), "escape collision fixture");
        assert_ne!(m1.cmp(&m2), std::cmp::Ordering::Equal);
        assert_ne!(f1, f2);
    }
}
