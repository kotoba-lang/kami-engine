//! Asset cache for loaded GPU meshes and materials.

use crate::{MaterialHandle, MeshHandle};
use std::collections::HashMap;

/// Cached GPU resource references keyed by string identifier (e.g. blob_key).
pub struct AssetCache {
    meshes: HashMap<String, (MeshHandle, u32)>, // handle + index_count
    materials: HashMap<String, MaterialHandle>,
}

impl AssetCache {
    pub fn new() -> Self {
        Self {
            meshes: HashMap::new(),
            materials: HashMap::new(),
        }
    }

    pub fn insert_mesh(&mut self, key: String, handle: MeshHandle, index_count: u32) {
        self.meshes.insert(key, (handle, index_count));
    }

    pub fn insert_material(&mut self, key: String, handle: MaterialHandle) {
        self.materials.insert(key, handle);
    }

    pub fn get_mesh(&self, key: &str) -> Option<(MeshHandle, u32)> {
        self.meshes.get(key).copied()
    }

    pub fn get_material(&self, key: &str) -> Option<MaterialHandle> {
        self.materials.get(key).copied()
    }

    pub fn has_mesh(&self, key: &str) -> bool {
        self.meshes.contains_key(key)
    }

    pub fn mesh_count(&self) -> usize {
        self.meshes.len()
    }

    pub fn material_count(&self) -> usize {
        self.materials.len()
    }
}

impl Default for AssetCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_insert_and_get() {
        let mut cache = AssetCache::new();
        cache.insert_mesh("cube".into(), MeshHandle(0), 36);
        cache.insert_material("default".into(), MaterialHandle(0));

        assert_eq!(cache.get_mesh("cube"), Some((MeshHandle(0), 36)));
        assert_eq!(cache.get_material("default"), Some(MaterialHandle(0)));
        assert_eq!(cache.get_mesh("missing"), None);
        assert!(cache.has_mesh("cube"));
        assert_eq!(cache.mesh_count(), 1);
    }
}
