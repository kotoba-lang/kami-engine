//! kami-mine-ai: AI-side heuristics for mining operations.
//!
//! Focuses on deterministic, explainable helpers that can run in edge/wasm:
//! - Extraction risk scoring
//! - Next-period extraction planning

use kami_mine_pds::{ExtractionRecord, Mine, MineStatus};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAssessment {
    pub score: u8,
    pub level: RiskLevel,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionPlan {
    pub target_tons: f64,
    pub current_tons: f64,
    pub remaining_tons: f64,
    pub suggested_next_period_tons: f64,
    pub actions: Vec<String>,
}

pub fn assess_extraction_risk(mine: &Mine, latest: &ExtractionRecord) -> RiskAssessment {
    let mut score: i32 = 30;
    let mut reasons = Vec::new();

    if mine.status != MineStatus::Active {
        score += 25;
        reasons.push("mine is not active".to_string());
    }

    if latest.quantity_tons > 250_000.0 {
        score += 20;
        reasons.push("high extraction throughput".to_string());
    }

    if latest.quantity_tons < 5_000.0 {
        score += 10;
        reasons.push("low throughput can indicate instability".to_string());
    }

    let grade = latest.grade.to_ascii_lowercase();
    if grade.contains("low") {
        score += 20;
        reasons.push("reported low ore grade".to_string());
    }
    if grade.contains("high") {
        score -= 10;
        reasons.push("reported high ore grade".to_string());
    }

    score = score.clamp(0, 100);
    let level = if score >= 70 {
        RiskLevel::High
    } else if score >= 40 {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    };

    RiskAssessment {
        score: score as u8,
        level,
        reasons,
    }
}

pub fn plan_next_extraction(
    target_tons: f64,
    current_tons: f64,
    risk: RiskLevel,
) -> ExtractionPlan {
    let safe_target = target_tons.max(0.0);
    let safe_current = current_tons.max(0.0);
    let remaining = (safe_target - safe_current).max(0.0);

    let pacing_factor = match risk {
        RiskLevel::High => 0.70,
        RiskLevel::Medium => 0.90,
        RiskLevel::Low => 1.10,
    };

    let suggested_next_period_tons = (remaining * pacing_factor).round();
    let actions = match risk {
        RiskLevel::High => vec![
            "Increase geotechnical inspection cadence".to_string(),
            "Run short-horizon simulation before blasting".to_string(),
            "Cap extraction ramp-up per shift".to_string(),
        ],
        RiskLevel::Medium => vec![
            "Monitor production variance weekly".to_string(),
            "Tune ore blending plan".to_string(),
        ],
        RiskLevel::Low => vec![
            "Maintain current extraction cadence".to_string(),
            "Continue monthly QA sampling".to_string(),
        ],
    };

    ExtractionPlan {
        target_tons: safe_target,
        current_tons: safe_current,
        remaining_tons: remaining,
        suggested_next_period_tons,
        actions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kami_mine_pds::{MineType, MineralType};

    fn sample_mine(status: MineStatus) -> Mine {
        Mine {
            mine_id: "mine-001".to_string(),
            name: "Sample".to_string(),
            mine_type: MineType::Surface,
            country: "AU".to_string(),
            region: "Pilbara".to_string(),
            status,
            area_hectares: 1000.0,
            operator: "KAMI Mining".to_string(),
        }
    }

    fn sample_record(quantity_tons: f64, grade: &str) -> ExtractionRecord {
        let _ = MineralType::Metallic;
        ExtractionRecord {
            record_id: "ext-001".to_string(),
            mine_id: "mine-001".to_string(),
            mineral_id: "min-iron".to_string(),
            period: "2026-Q1".to_string(),
            quantity_tons,
            grade: grade.to_string(),
        }
    }

    #[test]
    fn computes_high_risk_for_inactive_low_grade_mine() {
        let mine = sample_mine(MineStatus::Suspended);
        let rec = sample_record(300_000.0, "low");
        let risk = assess_extraction_risk(&mine, &rec);

        assert_eq!(risk.level, RiskLevel::High);
        assert!(risk.score >= 70);
    }

    #[test]
    fn plans_with_lower_pacing_for_high_risk() {
        let plan = plan_next_extraction(100_000.0, 40_000.0, RiskLevel::High);
        assert_eq!(plan.remaining_tons, 60_000.0);
        assert_eq!(plan.suggested_next_period_tons, 42_000.0);
    }
}
