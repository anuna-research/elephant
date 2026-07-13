//! `elephant dag` — the entire theory as a layered dependency graph.
//!
//! Nodes are literals (positive atoms) carrying their effective proof tag
//! (the same collapse as `elephant status`); every rule contributes edges
//! from its body literals to its head, so facts sit at the top and derived
//! conclusions flow downward. The layout is the hence `board --dag`
//! Sugiyama-style renderer (longest-path layering, median crossing
//! reduction, box-drawing edges with an ASCII fallback), made cycle-safe:
//! defeasible theories may legally contain cycles, which are broken for
//! layout and listed under the graph together with rule preferences.

use crate::cli::Ctx;
use crate::core::closure;
use crate::errors::AppResult;
use crate::queries::view;
use spindle_core::conclusion::ConclusionType;
use spindle_core::literal::Literal;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// Positive-atom node key: the literal's SPL form without negation or the
/// outer parens (`(blocked-by t "x")` → `blocked-by t "x"`).
fn node_key(lit: &Literal) -> String {
    let mut l = lit.clone();
    l.negation = false;
    let s = l.to_spl();
    s.strip_prefix('(')
        .and_then(|x| x.strip_suffix(')'))
        .unwrap_or(&s)
        .to_string()
}

struct Edge {
    from: String,
    to: String,
    rule: String,
    arrow: &'static str,
    body_negated: bool,
    head_negated: bool,
}

struct Graph {
    /// Sorted node set.
    nodes: Vec<String>,
    /// Effective tag per node (positive conclusions only); absent → `-d`.
    tags: BTreeMap<String, String>,
    /// Drawn dependencies: node → parents (deduped, self-loops excluded).
    deps: BTreeMap<String, BTreeSet<String>>,
    /// Full-fidelity edge list (one per rule × body literal).
    edges: Vec<Edge>,
    /// Rule preferences (superior, inferior).
    sups: Vec<(String, String)>,
}

fn build_graph(v: &crate::queries::View) -> Graph {
    let theory = &v.closure.theory;
    let mut nodes: BTreeSet<String> = BTreeSet::new();
    let mut deps: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut edges = Vec::new();

    for rule in theory.rules() {
        for head in &rule.head {
            let to = node_key(head);
            nodes.insert(to.clone());
            for b in rule.body.iter().filter_map(|b| b.as_logic()) {
                let from = node_key(&b.to_literal());
                nodes.insert(from.clone());
                if from != to {
                    deps.entry(to.clone()).or_default().insert(from.clone());
                }
                edges.push(Edge {
                    from,
                    to: to.clone(),
                    rule: rule.label.clone(),
                    arrow: rule.rule_type.arrow(),
                    body_negated: b.negation,
                    head_negated: head.negation,
                });
            }
        }
    }

    // Effective tag per positive atom, same precedence as `status`.
    fn rank(t: ConclusionType) -> u8 {
        match t {
            ConclusionType::DefinitelyProvable => 0,
            ConclusionType::DefeasiblyProvable => 1,
            ConclusionType::DefinitelyNotProvable => 2,
            ConclusionType::DefeasiblyNotProvable => 3,
        }
    }
    let mut best: BTreeMap<String, ConclusionType> = BTreeMap::new();
    for c in closure::presentable(&v.closure.conclusions) {
        if c.literal.negation {
            continue;
        }
        best.entry(node_key(&c.literal))
            .and_modify(|t| {
                if rank(c.conclusion_type) < rank(*t) {
                    *t = c.conclusion_type;
                }
            })
            .or_insert(c.conclusion_type);
    }
    let tags = best
        .into_iter()
        .map(|(k, t)| (k, t.symbol().to_string()))
        .collect();

    let sups = theory
        .superiorities()
        .iter()
        .map(|s| (s.superior.clone(), s.inferior.clone()))
        .collect();

    Graph {
        nodes: nodes.into_iter().collect(),
        tags,
        deps,
        edges,
        sups,
    }
}

// ---------------------------------------------------------------------------
// Cycle-safe layering
// ---------------------------------------------------------------------------

/// Longest-path layering via iterative DFS over dependencies. Edges that
/// close a cycle are dropped from the layout and returned as `(from, to)`.
fn layering(g: &Graph) -> (HashMap<String, usize>, Vec<(String, String)>) {
    #[derive(Clone, Copy, PartialEq)]
    enum St {
        New,
        Active,
        Done,
    }
    let dep_vec: HashMap<&str, Vec<&str>> = g
        .nodes
        .iter()
        .map(|n| {
            (
                n.as_str(),
                g.deps
                    .get(n)
                    .map(|s| s.iter().map(String::as_str).collect())
                    .unwrap_or_default(),
            )
        })
        .collect();
    let mut st: HashMap<&str, St> = g.nodes.iter().map(|n| (n.as_str(), St::New)).collect();
    let mut layer: HashMap<String, usize> = HashMap::new();
    let mut dropped: Vec<(String, String)> = Vec::new();

    for start in &g.nodes {
        if st[start.as_str()] != St::New {
            continue;
        }
        st.insert(start, St::Active);
        // (node, next dep index)
        let mut stack: Vec<(&str, usize)> = vec![(start.as_str(), 0)];
        while let Some(&(n, i)) = stack.last() {
            if let Some(&d) = dep_vec[n].get(i) {
                stack.last_mut().unwrap().1 += 1;
                match st[d] {
                    St::Done => {}
                    // `d` is an ancestor on the stack: a cycle. Skip the
                    // edge for layout; report it below the graph.
                    St::Active => dropped.push((d.to_string(), n.to_string())),
                    St::New => {
                        st.insert(d, St::Active);
                        stack.push((d, 0));
                    }
                }
            } else {
                stack.pop();
                // Deps still Active here are exactly the dropped ones.
                let l = dep_vec[n]
                    .iter()
                    .filter(|d| st[**d] == St::Done)
                    .map(|d| layer[*d] + 1)
                    .max()
                    .unwrap_or(0);
                layer.insert(n.to_string(), l);
                st.insert(n, St::Done);
            }
        }
    }
    (layer, dropped)
}

// ---------------------------------------------------------------------------
// Crossing reduction (median heuristic, two sweeps)
// ---------------------------------------------------------------------------

fn reduce_crossings(
    layers: &mut [Vec<&str>],
    deps: &BTreeMap<String, BTreeSet<String>>,
    children: &HashMap<&str, Vec<&str>>,
) {
    let max_layer = layers.len().saturating_sub(1);
    for _ in 0..2 {
        for level in 1..=max_layer {
            let parent_tasks = layers[level - 1].clone();
            let level_tasks = layers[level].clone();
            let mut medians: Vec<(&str, f64)> = level_tasks
                .iter()
                .map(|&node| {
                    let parents: Vec<f64> = deps
                        .get(node)
                        .map(|d| {
                            d.iter()
                                .filter_map(|p| slot_position(p.as_str(), &parent_tasks))
                                .collect()
                        })
                        .unwrap_or_default();
                    let med = if parents.is_empty() {
                        slot_position(node, &level_tasks).unwrap_or(0.0)
                    } else {
                        median(&parents)
                    };
                    (node, med)
                })
                .collect();
            medians.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            layers[level] = medians.into_iter().map(|(t, _)| t).collect();
        }
        for level in (0..max_layer).rev() {
            let child_tasks = layers[level + 1].clone();
            let level_tasks = layers[level].clone();
            let mut medians: Vec<(&str, f64)> = level_tasks
                .iter()
                .map(|&node| {
                    let kids: Vec<f64> = children
                        .get(node)
                        .map(|c| {
                            c.iter()
                                .filter_map(|&ch| slot_position(ch, &child_tasks))
                                .collect()
                        })
                        .unwrap_or_default();
                    let med = if kids.is_empty() {
                        slot_position(node, &level_tasks).unwrap_or(0.0)
                    } else {
                        median(&kids)
                    };
                    (node, med)
                })
                .collect();
            medians.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            layers[level] = medians.into_iter().map(|(t, _)| t).collect();
        }
    }
}

fn median(positions: &[f64]) -> f64 {
    let mut sorted = positions.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[mid]
    } else {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    }
}

fn slot_position(node: &str, layer: &[&str]) -> Option<f64> {
    layer.iter().position(|&t| t == node).map(|p| p as f64)
}

// ---------------------------------------------------------------------------
// Focus
// ---------------------------------------------------------------------------

/// Restrict the graph to one literal's cone: the focus node plus its
/// transitive dependencies and dependents. Edges with an endpoint outside
/// the cone are omitted (a dependent's unrelated other premises are not
/// pulled in).
fn focus_graph(g: Graph, key: &str) -> Graph {
    let mut children: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (node, parents) in &g.deps {
        for p in parents {
            children.entry(p.as_str()).or_default().insert(node);
        }
    }
    let mut keep: BTreeSet<&str> = BTreeSet::new();
    let mut queue: Vec<&str> = vec![key];
    while let Some(n) = queue.pop() {
        if keep.insert(n) {
            if let Some(parents) = g.deps.get(n) {
                queue.extend(parents.iter().map(String::as_str));
            }
        }
    }
    let mut queue: Vec<&str> = vec![key];
    let mut down: BTreeSet<&str> = BTreeSet::new();
    while let Some(n) = queue.pop() {
        if down.insert(n) {
            if let Some(kids) = children.get(n) {
                queue.extend(kids.iter());
            }
        }
    }
    keep.extend(down);

    let nodes: Vec<String> = g
        .nodes
        .iter()
        .filter(|n| keep.contains(n.as_str()))
        .cloned()
        .collect();
    let deps: BTreeMap<String, BTreeSet<String>> = g
        .deps
        .iter()
        .filter(|(n, _)| keep.contains(n.as_str()))
        .map(|(n, ps)| {
            (
                n.clone(),
                ps.iter()
                    .filter(|p| keep.contains(p.as_str()))
                    .cloned()
                    .collect(),
            )
        })
        .collect();
    let edges: Vec<Edge> = g
        .edges
        .into_iter()
        .filter(|e| keep.contains(e.from.as_str()) && keep.contains(e.to.as_str()))
        .collect();
    let rules: BTreeSet<&str> = edges.iter().map(|e| e.rule.as_str()).collect();
    let sups = g
        .sups
        .iter()
        .filter(|(s, i)| rules.contains(s.as_str()) || rules.contains(i.as_str()))
        .cloned()
        .collect();
    let tags = g
        .tags
        .into_iter()
        .filter(|(n, _)| keep.contains(n.as_str()))
        .collect();

    Graph {
        nodes,
        tags,
        deps,
        edges,
        sups,
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

pub fn dag(ctx: &Ctx, focus: Option<&str>) -> AppResult<()> {
    let v = view(ctx)?;
    let mut g = build_graph(&v);
    let total_literals = g.nodes.len();
    let focus_key = match focus {
        Some(f) => {
            let key = node_key(&crate::queries::parse_literal(f)?);
            if !g.nodes.iter().any(|n| n == &key) {
                return Err(crate::errors::AppError::NotFound(format!(
                    "no literal '{key}' in this theory's graph"
                )));
            }
            g = focus_graph(g, &key);
            Some(key)
        }
        None => None,
    };
    let (layer_of, cycles) = layering(&g);

    if ctx.json {
        let nodes: Vec<_> = g
            .nodes
            .iter()
            .map(|n| {
                serde_json::json!({
                    "literal": n,
                    "tag": g.tags.get(n).map(String::as_str).unwrap_or("-d"),
                    "layer": layer_of.get(n),
                })
            })
            .collect();
        let edges: Vec<_> = g
            .edges
            .iter()
            .map(|e| {
                serde_json::json!({
                    "from": e.from, "to": e.to, "rule": e.rule, "arrow": e.arrow,
                    "body_negated": e.body_negated, "head_negated": e.head_negated,
                })
            })
            .collect();
        let sups: Vec<_> = g
            .sups
            .iter()
            .map(|(s, i)| serde_json::json!({"superior": s, "inferior": i}))
            .collect();
        let cyc: Vec<_> = cycles
            .iter()
            .map(|(f, t)| serde_json::json!({"from": f, "to": t}))
            .collect();
        println!(
            "{}",
            serde_json::json!({"v":1, "theory": v.store.theory_id, "focus": focus_key,
                "nodes": nodes, "edges": edges, "superiorities": sups, "cycles": cyc})
        );
        return Ok(());
    }

    if g.nodes.is_empty() {
        println!("(empty theory)");
        return Ok(());
    }

    use std::io::IsTerminal as _;
    let tty = std::io::stdout().is_terminal();

    println!("Legend: +D definite  +d defeasible  -d not provable  -D definitely not");
    println!();

    // Children map (drawn edges only: deduped, minus cycle edges).
    let dropped: HashSet<(&str, &str)> = cycles
        .iter()
        .map(|(f, t)| (f.as_str(), t.as_str()))
        .collect();
    let mut children: HashMap<&str, Vec<&str>> = HashMap::new();
    for (node, parents) in &g.deps {
        for p in parents {
            if !dropped.contains(&(p.as_str(), node.as_str())) {
                children.entry(p.as_str()).or_default().push(node.as_str());
            }
        }
    }

    // Group nodes by layer, alphabetical before crossing reduction.
    let max_layer = layer_of.values().copied().max().unwrap_or(0);
    let mut layers: Vec<Vec<&str>> = vec![vec![]; max_layer + 1];
    for n in &g.nodes {
        layers[layer_of[n]].push(n.as_str());
    }
    for layer in &mut layers {
        layer.sort();
    }
    reduce_crossings(&mut layers, &g.deps, &children);

    let format_node = |n: &str| -> String {
        format!(
            "[{} {}]",
            g.tags.get(n).map(String::as_str).unwrap_or("-d"),
            n
        )
    };

    let max_node_width = g
        .nodes
        .iter()
        .map(|n| format_node(n).chars().count())
        .max()
        .unwrap_or(8)
        .max(8);
    let slot_width = max_node_width + 3;
    let max_layer_width = layers.iter().map(|l| l.len()).max().unwrap_or(1);
    let draw = DrawCtx {
        total_width: max_layer_width * slot_width,
        tty,
    };

    let mut node_pos: HashMap<&str, usize> = HashMap::new();
    for layer_nodes in &layers {
        for (slot, &n) in layer_nodes.iter().enumerate() {
            node_pos.insert(n, slot * slot_width + slot_width / 2);
        }
    }
    let center = |n: &str| node_pos[n];

    let layer_str: HashMap<&str, usize> = layer_of.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    for (layer_idx, layer_nodes) in layers.iter().enumerate() {
        let mut node_line = draw.make_line();
        for &n in layer_nodes {
            let s = format_node(n);
            let start = center(n).saturating_sub(s.chars().count() / 2);
            for (i, c) in s.chars().enumerate() {
                draw.set(&mut node_line, start + i, c);
            }
        }
        println!("{}", draw.rtrim(&node_line));
        if layer_idx < max_layer {
            draw_edges(layer_idx, &layers, &children, &layer_str, &center, &draw);
        }
    }

    if !cycles.is_empty() {
        println!();
        println!("cycles (not drawn):");
        for (f, t) in &cycles {
            println!("  {f} -> {t}");
        }
    }
    if !g.sups.is_empty() {
        println!();
        println!("preferences:");
        for (s, i) in &g.sups {
            println!("  {s} > {i}");
        }
    }
    println!();
    if let Some(key) = &focus_key {
        println!(
            "focus {key}: {} of {} literals",
            g.nodes.len(),
            total_literals
        );
    }
    let theory = &v.closure.theory;
    use spindle_core::rule::RuleType;
    println!(
        "{} literals, {} facts, {} strict, {} defeasible, {} defeaters, {} preferences",
        total_literals,
        theory.facts().count(),
        theory.rules_by_type(RuleType::Strict).count(),
        theory.rules_by_type(RuleType::Defeasible).count(),
        theory.rules_by_type(RuleType::Defeater).count(),
        theory.superiorities().len(),
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Edge drawing (ported from hence board --dag)
// ---------------------------------------------------------------------------

struct DrawCtx {
    total_width: usize,
    tty: bool,
}

impl DrawCtx {
    fn make_line(&self) -> Vec<char> {
        vec![' '; self.total_width]
    }

    fn set(&self, v: &mut [char], pos: usize, ch: char) {
        if pos < v.len() {
            v[pos] = ch;
        }
    }

    fn rtrim(&self, v: &[char]) -> String {
        let s: String = v.iter().collect();
        s.trim_end().to_string()
    }
}

#[derive(Clone, Copy, PartialEq)]
enum EdgeKind {
    Direct,
    Skip,
}

fn draw_edges(
    layer_idx: usize,
    layers: &[Vec<&str>],
    children: &HashMap<&str, Vec<&str>>,
    layer_of: &HashMap<&str, usize>,
    center: &dyn Fn(&str) -> usize,
    draw: &DrawCtx,
) {
    // Collect edges landing on the next layer: (from_center, to_center, kind)
    let mut edges: Vec<(usize, usize, EdgeKind)> = Vec::new();
    for &parent in &layers[layer_idx] {
        if let Some(kids) = children.get(parent) {
            for &child in kids {
                if layer_of.get(child) == Some(&(layer_idx + 1)) {
                    edges.push((center(parent), center(child), EdgeKind::Direct));
                }
            }
        }
    }
    for layer in &layers[..layer_idx] {
        for &parent in layer {
            if let Some(kids) = children.get(parent) {
                for &child in kids {
                    if layer_of.get(child) == Some(&(layer_idx + 1)) {
                        edges.push((center(parent), center(child), EdgeKind::Skip));
                    }
                }
            }
        }
    }

    // Pass-through positions (edges skipping this gap entirely)
    let mut pass_through: Vec<usize> = Vec::new();
    for layer in &layers[..=layer_idx] {
        for &parent in layer {
            if let Some(kids) = children.get(parent) {
                for &child in kids {
                    if let Some(&cl) = layer_of.get(child)
                        && cl > layer_idx + 1
                    {
                        let pos = center(parent);
                        if !pass_through.contains(&pos) {
                            pass_through.push(pos);
                        }
                    }
                }
            }
        }
    }

    if edges.is_empty() && pass_through.is_empty() {
        return;
    }

    let direct_from: HashSet<usize> = edges
        .iter()
        .filter(|e| e.2 == EdgeKind::Direct)
        .map(|e| e.0)
        .collect();
    let skip_from: HashSet<usize> = edges
        .iter()
        .filter(|e| e.2 == EdgeKind::Skip)
        .map(|e| e.0)
        .collect();
    let all_from: HashSet<usize> = direct_from.union(&skip_from).copied().collect();
    let all_to: HashSet<usize> = edges.iter().map(|e| e.1).collect();
    let all_straight = skip_from.is_empty() && edges.iter().all(|e| e.0 == e.1);

    let vert = if draw.tty { '\u{2502}' } else { '|' };
    let down = if draw.tty { '\u{2193}' } else { 'v' };

    if all_straight {
        let mut line1 = draw.make_line();
        let mut line2 = draw.make_line();
        for &pos in &pass_through {
            draw.set(&mut line1, pos, vert);
            draw.set(&mut line2, pos, vert);
        }
        for &from in &direct_from {
            draw.set(&mut line1, from, vert);
            draw.set(&mut line2, from, down);
        }
        println!("{}", draw.rtrim(&line1));
        println!("{}", draw.rtrim(&line2));
    } else {
        draw_routing_bar(&all_from, &all_to, &pass_through, draw);
    }
}

fn draw_routing_bar(
    all_from: &HashSet<usize>,
    all_to: &HashSet<usize>,
    pass_through: &[usize],
    draw: &DrawCtx,
) {
    let vert = if draw.tty { '\u{2502}' } else { '|' };
    let horiz = if draw.tty { '\u{2500}' } else { '-' };
    let down = if draw.tty { '\u{2193}' } else { 'v' };

    let bar_positions: HashSet<usize> = all_from.union(all_to).copied().collect();
    let min_pos = bar_positions.iter().copied().min().unwrap_or(0);
    let max_pos = bar_positions.iter().copied().max().unwrap_or(0);

    let pass_set: HashSet<usize> = pass_through.iter().copied().collect();
    let all_positions: HashSet<usize> = bar_positions.union(&pass_set).copied().collect();

    // Line 1: vertical drops from sources + pass-through
    let mut line1 = draw.make_line();
    for &pos in all_from {
        draw.set(&mut line1, pos, vert);
    }
    for &pos in pass_through {
        draw.set(&mut line1, pos, vert);
    }
    println!("{}", draw.rtrim(&line1));

    // Line 2: horizontal routing bar with junctions
    let mut line2 = draw.make_line();
    if min_pos != max_pos {
        for pos in min_pos..=max_pos {
            draw.set(&mut line2, pos, horiz);
        }
    }
    for &pos in &all_positions {
        let is_source = all_from.contains(&pos);
        let is_target = all_to.contains(&pos);
        let is_pass = pass_set.contains(&pos);
        let in_bar = pos >= min_pos && pos <= max_pos;
        let is_left = pos == min_pos;
        let is_right = pos == max_pos;
        let ch = junction_char(
            is_source, is_target, is_pass, in_bar, is_left, is_right, draw.tty,
        );
        draw.set(&mut line2, pos, ch);
    }
    println!("{}", draw.rtrim(&line2));

    // Line 3: arrows to targets + pass-through continues
    let mut line3 = draw.make_line();
    for &to in all_to {
        draw.set(&mut line3, to, down);
    }
    for &pos in pass_through {
        if !all_to.contains(&pos) {
            draw.set(&mut line3, pos, vert);
        }
    }
    println!("{}", draw.rtrim(&line3));
}

fn junction_char(
    is_source: bool,
    is_target: bool,
    is_pass: bool,
    in_bar: bool,
    is_left: bool,
    is_right: bool,
    tty: bool,
) -> char {
    if !tty {
        if is_pass && !is_source && !is_target {
            if !in_bar || (is_left && is_right) {
                '|'
            } else {
                '+'
            }
        } else if (is_source || is_target || is_pass) && !(is_left && is_right) {
            '+'
        } else if is_left && is_right {
            '|'
        } else {
            '-'
        }
    } else if is_pass && !is_source && !is_target {
        if !in_bar || (is_left && is_right) {
            '\u{2502}' // │
        } else {
            '\u{253c}' // ┼
        }
    } else if is_target && (is_source || is_pass) {
        if is_left && is_right {
            '\u{2502}'
        } else if is_left {
            '\u{251c}' // ├
        } else if is_right {
            '\u{2524}' // ┤
        } else {
            '\u{253c}' // ┼
        }
    } else if is_source {
        if is_left && is_right {
            '\u{2502}'
        } else if is_left {
            '\u{2514}' // └
        } else if is_right {
            '\u{2518}' // ┘
        } else {
            '\u{2534}' // ┴
        }
    } else if is_target {
        if is_left && is_right {
            '\u{2502}'
        } else if is_left {
            '\u{250c}' // ┌
        } else if is_right {
            '\u{2510}' // ┐
        } else {
            '\u{252c}' // ┬
        }
    } else {
        '\u{2500}' // ─
    }
}
