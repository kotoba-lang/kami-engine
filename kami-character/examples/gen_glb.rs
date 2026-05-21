/// Generate GLB from default CharacterDef.
fn main() {
    let def = kami_character::params::CharacterDef::default();
    let mesh = kami_character::generate_character(&def);
    let glb = kami_character::export::export_glb(&mesh, &def);
    let path = std::env::args().nth(1).unwrap_or_else(|| "/tmp/avatar-views/avatar_kami_character.glb".into());
    std::fs::write(&path, &glb).unwrap();
    let total_verts: usize = mesh.parts.iter().map(|p| p.vertices.len()).sum();
    let total_tris: usize = mesh.parts.iter().map(|p| p.indices.len() / 3).sum();
    println!("GLB: {} ({} bytes, {} verts, {} tris, {} parts)",
        path, glb.len(), total_verts, total_tris, mesh.parts.len());
}
