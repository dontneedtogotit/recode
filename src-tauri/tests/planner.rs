use recode::planner::{LocalPlanner, Planner};

#[test]
fn planner_creates_plan_from_goal() {
    let planner = LocalPlanner::new();
    let plan = planner.create_plan("build landing page").expect("plan");
    assert_eq!(plan.goal, "build landing page");
    assert!(!plan.steps.is_empty());
}
