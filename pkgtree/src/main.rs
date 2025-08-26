use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use clap::{ArgAction, Parser, ValueEnum};
use miette::{Diagnostic, IntoDiagnostic, Result};
use thiserror::Error;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use libips::image::Image;

#[derive(Parser, Debug)]
#[command(name = "pkgtree", version, about = "Analyze IPS package dependency trees and detect cycles", long_about = None)]
struct Cli {
    /// Path to an IPS image (root containing var/pkg)
    #[arg(short = 'I', long = "image", env = "IPS_IMAGE")] 
    image_path: PathBuf,

    /// Publisher to analyze (default: all publishers in the image)
    #[arg(short = 'P', long)]
    publisher: Option<String>,

    /// Only analyze packages whose stem or FMRI contains this substring (case sensitive)
    #[arg(short = 'n', long)]
    package: Option<String>,

    /// Output format
    #[arg(short = 'F', long = "format", default_value_t = OutputFormat::Tree)]
    format: OutputFormat,

    /// Maximum depth to print for the tree (0 = unlimited)
    #[arg(short = 'd', long = "max-depth", default_value_t = 0)]
    max_depth: usize,

    /// Detect and report dependency cycles across the analyzed set
    #[arg(short = 'c', long = "detect-cycles", action = ArgAction::SetTrue)]
    detect_cycles: bool,

    /// Emit suggestions to break detected cycles
    #[arg(short = 's', long = "suggest", action = ArgAction::SetTrue)]
    suggest: bool,

    /// Increase log verbosity (use multiple times)
    #[arg(short = 'v', long = "verbose", action = ArgAction::Count)]
    verbose: u8,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum OutputFormat {
    Tree,
    Json,
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputFormat::Tree => write!(f, "tree"),
            OutputFormat::Json => write!(f, "json"),
        }
    }
}

#[derive(Error, Debug, Diagnostic)]
#[error("pkgtree error: {message}")]
#[diagnostic(code(ips::pkgtree_error), help("See logs with RUST_LOG=pkgtree=debug for more details."))]
struct PkgTreeError {
    message: String,
}

#[derive(Debug, Clone)]
struct Edge {
    to: String,         // target stem
    dep_type: String,   // dependency type (e.g., require, incorporate, optional, etc.)
}

#[derive(Debug, Default, Clone)]
struct Graph {
    // stem -> edges
    adj: HashMap<String, Vec<Edge>>,
}

impl Graph {
    fn add_edge(&mut self, from: String, to: String, dep_type: String) {
        self.adj.entry(from).or_default().push(Edge { to, dep_type });
    }

    fn stems(&self) -> impl Iterator<Item = &String> {
        self.adj.keys()
    }
}

#[derive(Debug, Clone)]
struct Cycle {
    nodes: Vec<String>, // ordered stems forming the cycle, first == last for readability
    edges: Vec<String>, // edge types along the cycle
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Setup tracing
    let env_filter = match cli.verbose {
        0 => EnvFilter::from_default_env().add_directive("pkgtree=info".parse().unwrap()),
        1 => EnvFilter::from_default_env().add_directive("pkgtree=debug".parse().unwrap()),
        _ => EnvFilter::from_default_env().add_directive("pkgtree=trace".parse().unwrap()),
    };
    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    // Load image
    let image = Image::load(&cli.image_path).map_err(|e| PkgTreeError { message: format!("Failed to load image at {:?}: {}", cli.image_path, e) })?;

    // Query catalog (filtered if --package provided)
    let mut pkgs = if let Some(ref needle) = cli.package {
        image.query_catalog(Some(needle.as_str())).map_err(|e| PkgTreeError { message: format!("Failed to query catalog: {}", e) })?
    } else {
        image.query_catalog(None).map_err(|e| PkgTreeError { message: format!("Failed to query catalog: {}", e) })?
    };

    // Filter by publisher if specified
    if let Some(pubname) = &cli.publisher {
        pkgs.retain(|p| p.publisher == *pubname);
    }

    // Select starting set by package substring if requested
    let filter_substr = cli.package.clone();

    // Build dependency graph from manifests
    let mut graph = Graph::default();

    for p in &pkgs {
        // If filter is set and neither stem nor fmri string contains it, skip
        if let Some(ref needle) = filter_substr {
            let stem = p.fmri.stem().to_string();
            let fmri_str = p.fmri.to_string();
            if !stem.contains(needle) && !fmri_str.contains(needle) {
                continue;
            }
        }

        // Get manifest
        match image.get_manifest_from_catalog(&p.fmri) {
            Ok(Some(manifest)) => {
                let from_stem = p.fmri.stem().to_string();
                for dep in manifest.dependencies {
                    if let Some(dep_fmri) = dep.fmri {
                        let to_stem = dep_fmri.stem().to_string();
                        graph.add_edge(from_stem.clone(), to_stem, dep.dependency_type.clone());
                    }
                }
            }
            Ok(None) => {
                warn!(fmri=%p.fmri.to_string(), "Manifest not found in catalog");
            }
            Err(err) => {
                warn!(error=%format!("{}", err), fmri=%p.fmri.to_string(), "Failed to get manifest from catalog");
            }
        }
    }

    // If no nodes were added (e.g., filter too narrow), try building graph for all packages to support cycle analysis
    if graph.adj.is_empty() && filter_substr.is_some() {
        info!("No packages matched filter for dependency graph; analyzing full catalog for cycles/tree context.");
        for p in &pkgs {
            match image.get_manifest_from_catalog(&p.fmri) {
                Ok(Some(manifest)) => {
                    let from_stem = p.fmri.stem().to_string();
                    for dep in manifest.dependencies {
                        if let Some(dep_fmri) = dep.fmri {
                            let to_stem = dep_fmri.stem().to_string();
                            graph.add_edge(from_stem.clone(), to_stem, dep.dependency_type.clone());
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Determine roots for tree printing
    let roots: Vec<String> = if let Some(ref needle) = filter_substr {
        let mut r = HashSet::new();
        for k in graph.adj.keys() {
            if k.contains(needle) { r.insert(k.clone()); }
        }
        r.into_iter().collect()
    } else {
        graph.adj.keys().cloned().collect()
    };

    // Optionally detect cycles
    let mut cycles: Vec<Cycle> = Vec::new();
    if cli.detect_cycles {
        cycles = detect_cycles(&graph);
    }

    match cli.format {
        OutputFormat::Tree => {
            print_trees(&graph, &roots, cli.max_depth);
            if cli.detect_cycles {
                print_cycles(&cycles);
                if cli.suggest {
                    print_suggestions(&cycles, &graph);
                }
            }
        }
        OutputFormat::Json => {
            use serde::Serialize;
            #[derive(Serialize)]
            struct JsonEdge { from: String, to: String, dep_type: String }
            #[derive(Serialize)]
            struct JsonCycle { nodes: Vec<String>, edges: Vec<String> }
            #[derive(Serialize)]
            struct Payload { edges: Vec<JsonEdge>, cycles: Vec<JsonCycle> }

            let mut edges = Vec::new();
            for (from, es) in &graph.adj {
                for e in es { edges.push(JsonEdge{ from: from.clone(), to: e.to.clone(), dep_type: e.dep_type.clone() }); }
            }
            let cycles_json = cycles.iter().map(|c| JsonCycle { nodes: c.nodes.clone(), edges: c.edges.clone() }).collect();
            let payload = Payload { edges, cycles: cycles_json };
            println!("{}", serde_json::to_string_pretty(&payload).into_diagnostic()?);
        }
    }

    Ok(())
}

fn print_trees(graph: &Graph, roots: &[String], max_depth: usize) {
    // Print a tree for each root
    let mut printed = HashSet::new();
    for r in roots {
        if printed.contains(r) { continue; }
        printed.insert(r.clone());
        println!("{}", r);
        let mut path = Vec::new();
        let mut seen = HashSet::new();
        print_tree_rec(graph, r, 1, max_depth, &mut path, &mut seen);
        println!("");
    }
}

fn print_tree_rec(
    graph: &Graph,
    node: &str,
    depth: usize,
    max_depth: usize,
    path: &mut Vec<String>,
    seen: &mut HashSet<String>,
) {
    if max_depth != 0 && depth > max_depth { return; }
    path.push(node.to_string());
    seen.insert(node.to_string());

    if let Some(edges) = graph.adj.get(node) {
        for e in edges {
            let last = if path.contains(&e.to) { " (cycle)" } else { "" };
            println!("{}└─ {} [{}]{}", "  ".repeat(depth), e.to, e.dep_type, last);
            if !path.contains(&e.to) {
                print_tree_rec(graph, &e.to, depth + 1, max_depth, path, seen);
            }
        }
    }

    path.pop();
}

fn detect_cycles(graph: &Graph) -> Vec<Cycle> {
    let mut visited: HashSet<String> = HashSet::new();
    let mut stack: Vec<String> = Vec::new();
    let mut cycles = Vec::new();

    for node in graph.stems().cloned().collect::<Vec<_>>() {
        if !visited.contains(&node) {
            dfs_cycles(graph, &node, &mut visited, &mut stack, &mut cycles);
        }
    }

    dedup_cycles(cycles)
}

fn dfs_cycles(
    graph: &Graph,
    node: &str,
    visited: &mut HashSet<String>,
    stack: &mut Vec<String>,
    cycles: &mut Vec<Cycle>,
) {
    visited.insert(node.to_string());
    stack.push(node.to_string());

    if let Some(edges) = graph.adj.get(node) {
        for e in edges {
            let to = &e.to;
            if let Some(pos) = stack.iter().position(|n| n == to) {
                // Found a cycle: stack[pos..] -> to
                let mut cycle_nodes = stack[pos..].to_vec();
                cycle_nodes.push(to.clone());
                let mut cycle_edges = Vec::new();
                for i in pos..stack.len() { 
                    let from = &stack[i];
                    let to2 = if i + 1 < stack.len() { &stack[i+1] } else { to };
                    if let Some(es2) = graph.adj.get(from) {
                        if let Some(edge) = es2.iter().find(|ed| &ed.to == to2) {
                            cycle_edges.push(edge.dep_type.clone());
                        } else {
                            cycle_edges.push("unknown".to_string());
                        }
                    }
                }
                cycles.push(Cycle { nodes: cycle_nodes, edges: cycle_edges });
            } else if !visited.contains(to) {
                dfs_cycles(graph, to, visited, stack, cycles);
            }
        }
    }

    stack.pop();
}

fn dedup_cycles(mut cycles: Vec<Cycle>) -> Vec<Cycle> {
    // Normalize cycles so that smallest node lexicographically is first, and ensure start==end
    for c in cycles.iter_mut() {
        if c.nodes.first() != c.nodes.last() && !c.nodes.is_empty() {
            c.nodes.push(c.nodes.first().unwrap().clone());
        }
        // rotate to minimal node position (excluding the duplicate last element when comparing)
        if c.nodes.len() > 1 {
            let inner = &c.nodes[..c.nodes.len()-1];
            if let Some((min_idx, _)) = inner.iter().enumerate().min_by_key(|(_, n)| *n) {
                c.nodes.rotate_left(min_idx);
                c.edges.rotate_left(min_idx);
            }
        }
    }
    // Deduplicate by string key
    let mut seen = HashSet::new();
    cycles.retain(|c| {
        let key = c.nodes.join("->");
        if seen.contains(&key) { false } else { seen.insert(key); true }
    });
    cycles
}

fn print_cycles(cycles: &[Cycle]) {
    if cycles.is_empty() {
        println!("No dependency cycles detected.");
        return;
    }
    println!("Detected {} cycle(s):", cycles.len());
    for (i, c) in cycles.iter().enumerate() {
        println!("  {}. {}", i + 1, c.nodes.join(" -> "));
    }
}

fn print_suggestions(cycles: &[Cycle], graph: &Graph) {
    if cycles.is_empty() { return; }
    println!("\nSuggestions to break cycles (heuristic):");
    for (i, c) in cycles.iter().enumerate() {
        // Prefer breaking an 'incorporate' edge if present, otherwise any edge
        let mut suggested: Option<(String, String)> = None; // (from, to)
        'outer: for w in c.nodes.windows(2) {
            let from = &w[0];
            let to = &w[1];
            if let Some(es) = graph.adj.get(from) {
                for e in es {
                    if &e.to == to {
                        if e.dep_type == "incorporate" { suggested = Some((from.clone(), to.clone())); break 'outer; }
                        if suggested.is_none() { suggested = Some((from.clone(), to.clone())); }
                    }
                }
            }
        }
        if let Some((from, to)) = suggested {
            println!("  {}. Consider relaxing/removing edge {} -> {} (preferably if it's an incorporation).", i + 1, from, to);
        } else {
            println!("  {}. Consider relaxing one edge along the cycle: {}", i + 1, c.nodes.join(" -> "));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_simple_cycle() {
        let mut g = Graph::default();
        g.add_edge("A".to_string(), "B".to_string(), "require".to_string());
        g.add_edge("B".to_string(), "C".to_string(), "require".to_string());
        g.add_edge("C".to_string(), "A".to_string(), "incorporate".to_string());
        let cycles = detect_cycles(&g);
        assert!(!cycles.is_empty());
    }
}
