mod conflicts;
mod fixture;
mod item;
mod kinds;
mod updates;

use super::{PlannedChange, PreparedPlan};
use forgearr_shared::plugins::store::plan::{PlanAction, PlanItem, PlanTool};

const OFFICIAL: &str = "repo.official";
const THIRD_PARTY: &str = "repo.tiers";

/// Éléments du plan, dans leur ordre.
fn actions(prepared: &PreparedPlan) -> Vec<(&str, PlanAction)> {
    let items = prepared.plan.items.iter();
    items.map(|item| (item.id.as_str(), item.action)).collect()
}

fn item<'a>(prepared: &'a PreparedPlan, id: &str) -> anyhow::Result<&'a PlanItem> {
    let found = prepared.plan.items.iter().find(|item| item.id == id);
    found.ok_or_else(|| anyhow::anyhow!("{id} absent du plan"))
}

fn tool<'a>(prepared: &'a PreparedPlan, id: &str) -> anyhow::Result<&'a PlanTool> {
    let found = prepared.plan.tools.iter().find(|tool| tool.id == id);
    found.ok_or_else(|| anyhow::anyhow!("outil {id} absent du plan"))
}

fn change<'a>(prepared: &'a PreparedPlan, id: &str) -> anyhow::Result<&'a PlannedChange> {
    let found = prepared
        .changes
        .iter()
        .find(|change| change.content.entry.id == id);
    found.ok_or_else(|| anyhow::anyhow!("{id} ne change pas"))
}

/// Rien à refuser, à accepter ni à signaler.
fn ensure_plain(prepared: &PreparedPlan) -> anyhow::Result<()> {
    let plan = &prepared.plan;
    anyhow::ensure!(plan.blockers.is_empty(), "{:?}", plan.blockers);
    anyhow::ensure!(plan.consents.is_empty(), "{:?}", plan.consents);
    anyhow::ensure!(plan.warnings.is_empty(), "{:?}", plan.warnings);
    Ok(())
}
