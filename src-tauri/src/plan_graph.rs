use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModuleStatus {
    Pending,
    Authoring,
    Verified,
    AuthorFailed,
    Generating,
    Done,
    GenFailed,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlanTask {
    pub id: String,
    pub produces: Vec<String>,
    pub consumes: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModuleGraph {
    pub tasks: Vec<PlanTask>,
    pub task_dependencies: HashMap<String, Vec<String>>,
    pub cycles: Vec<Vec<String>>,
}

pub struct PlanGraphEngine;

impl PlanGraphEngine {
    /// Build a task-level dependency graph from declared produces/consumes.
    ///
    /// For each task, returns the task IDs that produce symbols it consumes.
    /// A consumed symbol with no producer is silently skipped.
    pub fn build_module_dependency_graph(tasks: &[PlanTask]) -> HashMap<String, Vec<String>> {
        let producers_of: HashMap<String, Vec<String>> = {
            let mut map: HashMap<String, Vec<String>> = HashMap::new();
            for task in tasks {
                for symbol in &task.produces {
                    map.entry(symbol.clone())
                        .or_default()
                        .push(task.id.clone());
                }
            }
            map
        };

        let mut graph = HashMap::new();
        for task in tasks {
            let mut deps = HashSet::new();
            for symbol in &task.consumes {
                for producer_id in producers_of.get(symbol).unwrap_or(&Vec::new()) {
                    if producer_id != &task.id {
                        deps.insert(producer_id.clone());
                    }
                }
            }
            let mut sorted_deps: Vec<String> = deps.into_iter().collect();
            sorted_deps.sort();
            graph.insert(task.id.clone(), sorted_deps);
        }
        graph
    }

    /// Detect cycles in the module graph using DFS.
    ///
    /// Returns every cycle found as an ordered list of module IDs.
    /// The last element repeats the first to close the cycle.
    /// Empty array means the graph is acyclic.
    pub fn find_module_graph_cycles(graph: &HashMap<String, Vec<String>>) -> Vec<Vec<String>> {
        let mut cycles = Vec::new();
        let mut visited = HashSet::new();
        let mut stack = Vec::new();
        let mut on_stack = HashSet::new();

        fn visit(
            node: &str,
            graph: &HashMap<String, Vec<String>>,
            visited: &mut HashSet<String>,
            stack: &mut Vec<String>,
            on_stack: &mut HashSet<String>,
            cycles: &mut Vec<Vec<String>>,
        ) {
            visited.insert(node.to_string());
            stack.push(node.to_string());
            on_stack.insert(node.to_string());

            for dep in graph.get(node).unwrap_or(&Vec::new()) {
                if !visited.contains(dep) {
                    visit(dep, graph, visited, stack, on_stack, cycles);
                } else if on_stack.contains(dep) {
                    if let Some(cycle_start) = stack.iter().position(|id| id == dep) {
                        let mut cycle = stack[cycle_start..].to_vec();
                        cycle.push(dep.clone());
                        cycles.push(cycle);
                    }
                }
            }

            stack.pop();
            on_stack.remove(node);
        }

        for node in graph.keys() {
            if !visited.contains(node.as_str()) {
                visit(node, graph, &mut visited, &mut stack, &mut on_stack, &mut cycles);
            }
        }

        cycles
    }

    /// Build the module graph and check for cycles in one step.
    pub fn analyze(tasks: &[PlanTask]) -> ModuleGraph {
        let task_dependencies = Self::build_module_dependency_graph(tasks);
        let cycles = Self::find_module_graph_cycles(&task_dependencies);
        ModuleGraph {
            tasks: tasks.to_vec(),
            task_dependencies,
            cycles,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tasks() -> Vec<PlanTask> {
        vec![
            PlanTask {
                id: "task-a".into(),
                produces: vec!["symbol-1".into()],
                consumes: vec![],
            },
            PlanTask {
                id: "task-b".into(),
                produces: vec!["symbol-2".into()],
                consumes: vec!["symbol-1".into()],
            },
            PlanTask {
                id: "task-c".into(),
                produces: vec![],
                consumes: vec!["symbol-1".into(), "symbol-2".into()],
            },
        ]
    }

    fn cyclic_tasks() -> Vec<PlanTask> {
        vec![
            PlanTask {
                id: "task-x".into(),
                produces: vec!["sx".into()],
                consumes: vec!["sy".into()],
            },
            PlanTask {
                id: "task-y".into(),
                produces: vec!["sy".into()],
                consumes: vec!["sx".into()],
            },
        ]
    }

    #[test]
    fn build_module_dependency_graph_resolves_producers() {
        let tasks = sample_tasks();
        let graph = PlanGraphEngine::build_module_dependency_graph(&tasks);

        assert_eq!(graph.get("task-b"), Some(&vec!["task-a".into()]));
        assert_eq!(graph.get("task-c"), Some(&vec!["task-a".into(), "task-b".into()]));
        assert_eq!(graph.get("task-a"), Some(&vec![] as &Vec<String>));
    }

    #[test]
    fn find_module_graph_cycles_detects_cycle() {
        let tasks = cyclic_tasks();
        let graph = PlanGraphEngine::build_module_dependency_graph(&tasks);
        let cycles = PlanGraphEngine::find_module_graph_cycles(&graph);

        assert_eq!(cycles.len(), 1);
        assert!(cycles[0].contains(&"task-x".into()));
        assert!(cycles[0].contains(&"task-y".into()));
    }

    #[test]
    fn find_module_graph_cycles_returns_empty_for_dag() {
        let tasks = sample_tasks();
        let graph = PlanGraphEngine::build_module_dependency_graph(&tasks);
        let cycles = PlanGraphEngine::find_module_graph_cycles(&graph);

        assert!(cycles.is_empty());
    }

    #[test]
    fn analyze_returns_graph_with_no_cycles() {
        let tasks = sample_tasks();
        let module_graph = PlanGraphEngine::analyze(&tasks);

        assert!(module_graph.cycles.is_empty());
        assert_eq!(module_graph.task_dependencies.get("task-c"), Some(&vec!["task-a".into(), "task-b".into()]));
    }

    #[test]
    fn analyze_detects_cycles() {
        let tasks = cyclic_tasks();
        let module_graph = PlanGraphEngine::analyze(&tasks);

        assert_eq!(module_graph.cycles.len(), 1);
    }

    #[test]
    fn missing_producer_is_skipped() {
        let tasks = vec![PlanTask {
            id: "task-1".into(),
            produces: vec![],
            consumes: vec!["missing-symbol".into()],
        }];
        let graph = PlanGraphEngine::build_module_dependency_graph(&tasks);

        assert_eq!(graph.get("task-1"), Some(&vec![] as &Vec<String>));
    }
}
