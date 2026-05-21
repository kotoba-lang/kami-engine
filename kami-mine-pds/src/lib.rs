//! kami-mine-pds: Mining domain PDS primitives for KAMI Engine.
//!
//! Provides in-memory registry/ledger building blocks:
//! - Mine registry
//! - Mineral catalog
//! - Extraction history ledger

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MineType {
    Surface,
    Underground,
    Placer,
    Dredging,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MineralType {
    Metallic,
    NonMetallic,
    Energy,
    Gemstone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MineStatus {
    Active,
    Suspended,
    Abandoned,
    Reclaimed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mine {
    pub mine_id: String,
    pub name: String,
    pub mine_type: MineType,
    pub country: String,
    pub region: String,
    pub status: MineStatus,
    pub area_hectares: f64,
    pub operator: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mineral {
    pub mineral_id: String,
    pub name: String,
    pub mineral_type: MineralType,
    pub chemical_formula: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionRecord {
    pub record_id: String,
    pub mine_id: String,
    pub mineral_id: String,
    pub period: String,
    pub quantity_tons: f64,
    pub grade: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MinePdsError {
    #[error("mine already exists: {0}")]
    MineAlreadyExists(String),
    #[error("mine not found: {0}")]
    MineNotFound(String),
    #[error("mineral already exists: {0}")]
    MineralAlreadyExists(String),
    #[error("mineral not found: {0}")]
    MineralNotFound(String),
    #[error("invalid extraction quantity: {0}")]
    InvalidQuantity(String),
}

#[derive(Debug, Default)]
pub struct MineLedger {
    mines: HashMap<String, Mine>,
    minerals: HashMap<String, Mineral>,
    extraction_records: Vec<ExtractionRecord>,
}

impl MineLedger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_mine(&mut self, mine: Mine) -> Result<(), MinePdsError> {
        if self.mines.contains_key(&mine.mine_id) {
            return Err(MinePdsError::MineAlreadyExists(mine.mine_id));
        }
        self.mines.insert(mine.mine_id.clone(), mine);
        Ok(())
    }

    pub fn update_mine_status(
        &mut self,
        mine_id: &str,
        status: MineStatus,
    ) -> Result<(), MinePdsError> {
        let mine = self
            .mines
            .get_mut(mine_id)
            .ok_or_else(|| MinePdsError::MineNotFound(mine_id.to_string()))?;
        mine.status = status;
        Ok(())
    }

    pub fn register_mineral(&mut self, mineral: Mineral) -> Result<(), MinePdsError> {
        if self.minerals.contains_key(&mineral.mineral_id) {
            return Err(MinePdsError::MineralAlreadyExists(mineral.mineral_id));
        }
        self.minerals.insert(mineral.mineral_id.clone(), mineral);
        Ok(())
    }

    pub fn record_extraction(&mut self, record: ExtractionRecord) -> Result<(), MinePdsError> {
        if !self.mines.contains_key(&record.mine_id) {
            return Err(MinePdsError::MineNotFound(record.mine_id));
        }
        if !self.minerals.contains_key(&record.mineral_id) {
            return Err(MinePdsError::MineralNotFound(record.mineral_id));
        }
        if !(record.quantity_tons.is_finite() && record.quantity_tons >= 0.0) {
            return Err(MinePdsError::InvalidQuantity(record.quantity_tons.to_string()));
        }

        self.extraction_records.push(record);
        Ok(())
    }

    pub fn get_mine(&self, mine_id: &str) -> Option<&Mine> {
        self.mines.get(mine_id)
    }

    pub fn get_mineral(&self, mineral_id: &str) -> Option<&Mineral> {
        self.minerals.get(mineral_id)
    }

    pub fn list_mines(&self) -> Vec<&Mine> {
        self.mines.values().collect()
    }

    pub fn extraction_history(&self, mine_id: &str) -> Vec<&ExtractionRecord> {
        self.extraction_records
            .iter()
            .filter(|r| r.mine_id == mine_id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_mine() -> Mine {
        Mine {
            mine_id: "mine-001".to_string(),
            name: "Pilbara East".to_string(),
            mine_type: MineType::Surface,
            country: "AU".to_string(),
            region: "Pilbara".to_string(),
            status: MineStatus::Active,
            area_hectares: 1200.0,
            operator: "KAMI Mining".to_string(),
        }
    }

    fn sample_mineral() -> Mineral {
        Mineral {
            mineral_id: "min-iron".to_string(),
            name: "Iron Ore".to_string(),
            mineral_type: MineralType::Metallic,
            chemical_formula: "Fe2O3".to_string(),
        }
    }

    #[test]
    fn registers_mine_and_mineral_and_records_extraction() {
        let mut ledger = MineLedger::new();
        ledger.register_mine(sample_mine()).unwrap();
        ledger.register_mineral(sample_mineral()).unwrap();

        ledger
            .record_extraction(ExtractionRecord {
                record_id: "ext-001".to_string(),
                mine_id: "mine-001".to_string(),
                mineral_id: "min-iron".to_string(),
                period: "2026-Q1".to_string(),
                quantity_tons: 250_000.0,
                grade: "62% Fe".to_string(),
            })
            .unwrap();

        assert_eq!(ledger.list_mines().len(), 1);
        assert_eq!(ledger.extraction_history("mine-001").len(), 1);
    }

    #[test]
    fn rejects_extraction_for_unknown_mine() {
        let mut ledger = MineLedger::new();
        ledger.register_mineral(sample_mineral()).unwrap();

        let err = ledger
            .record_extraction(ExtractionRecord {
                record_id: "ext-001".to_string(),
                mine_id: "missing".to_string(),
                mineral_id: "min-iron".to_string(),
                period: "2026-Q1".to_string(),
                quantity_tons: 1.0,
                grade: "low".to_string(),
            })
            .unwrap_err();

        assert_eq!(err, MinePdsError::MineNotFound("missing".to_string()));
    }
}
