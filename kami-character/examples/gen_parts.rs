/// Generate all VRM part GLB files for image2vrm part composition.
/// Covers all 21 hair presets × 3 colors + all 11 clothing presets × 2 colors.
fn main() {
    use kami_character::hair;
    use kami_character::body;
    use kami_character::params::*;
    use kami_character::export;
    use kami_character::CharacterMesh;

    let out_dir = std::env::args().nth(1).unwrap_or_else(|| "/tmp/avatar-parts".into());
    std::fs::create_dir_all(&out_dir).unwrap();

    let colors: Vec<(&str, [f32; 3])> = vec![
        ("black",   [0.08, 0.06, 0.05]),
        ("brown",   [0.40, 0.25, 0.15]),
        ("blonde",  [0.92, 0.85, 0.70]),
        ("red",     [0.70, 0.20, 0.12]),
        ("pink",    [0.95, 0.45, 0.55]),
        ("blue",    [0.15, 0.30, 0.80]),
        ("silver",  [0.78, 0.78, 0.82]),
        ("green",   [0.15, 0.55, 0.30]),
    ];

    let hair_presets: Vec<(&str, HairPreset)> = vec![
        ("short_straight",  HairPreset::ShortStraight),
        ("short_wavy",      HairPreset::ShortWavy),
        ("short_curly",     HairPreset::ShortCurly),
        ("medium_straight", HairPreset::MediumStraight),
        ("medium_wavy",     HairPreset::MediumWavy),
        ("medium_layered",  HairPreset::MediumLayered),
        ("long_straight",   HairPreset::LongStraight),
        ("long_wavy",       HairPreset::LongWavy),
        ("long_curly",      HairPreset::LongCurly),
        ("ponytail_high",   HairPreset::PonytailHigh),
        ("ponytail_low",    HairPreset::PonytailLow),
        ("bun_top",         HairPreset::BunTop),
        ("bun_low",         HairPreset::BunLow),
        ("bob",             HairPreset::Bob),
        ("pixie",           HairPreset::Pixie),
        ("buzz",            HairPreset::Buzz),
        ("undercut",        HairPreset::Undercut),
        ("mohawk",          HairPreset::Mohawk),
        ("afro_short",      HairPreset::AfroShort),
        ("afro_large",      HairPreset::AfroLarge),
        ("braids_twin",     HairPreset::BraidsTwin),
        ("braids_single",   HairPreset::BraidsSingle),
    ];

    // Generate all hair preset × color combinations
    let mut count = 0;
    for (preset_name, preset) in &hair_presets {
        for (color_name, color) in &colors {
            let name = format!("hair_{preset_name}_{color_name}");
            let params = HairParams {
                preset: *preset,
                color: *color,
                highlight_color: None,
                length_scale: 1.0,
                volume: 0.6,
                part_position: 0.5,
                shininess: 0.6,
            };
            let part = hair::generate_hair(&params);
            let mesh = CharacterMesh { parts: vec![part], skeleton: None, blendshape_targets: vec![] };
            let def = CharacterDef { hair: params, ..Default::default() };
            let glb = export::export_glb(&mesh, &def);
            let path = format!("{out_dir}/{name}.glb");
            std::fs::write(&path, &glb).unwrap();
            count += 1;
        }
    }
    println!("Hair: {count} files");

    // ── Outfit variants ──
    let body_params = BodyParams { height: 1.0, shoulder_width: 0.4, build: 0.3, neck_thickness: 0.4 };

    let outfit_colors: Vec<(&str, [f32; 3], Option<[f32; 3]>)> = vec![
        ("white",     [0.95, 0.95, 0.95], None),
        ("black",     [0.10, 0.10, 0.12], None),
        ("navy",      [0.10, 0.15, 0.35], None),
        ("red",       [0.75, 0.15, 0.15], None),
        ("pink",      [0.95, 0.60, 0.70], None),
        ("gray",      [0.45, 0.45, 0.48], None),
        ("beige",     [0.85, 0.78, 0.65], None),
        ("green",     [0.20, 0.50, 0.30], None),
    ];

    let outfit_presets: Vec<(&str, ClothingPreset)> = vec![
        ("tank_top",         ClothingPreset::TankTop),
        ("tshirt",           ClothingPreset::TShirt),
        ("blouse",           ClothingPreset::Blouse),
        ("hoodie",           ClothingPreset::Hoodie),
        ("jacket",           ClothingPreset::Jacket),
        ("dress_casual",     ClothingPreset::DressCasual),
        ("dress_formal",     ClothingPreset::DressFormal),
        ("suit_casual",      ClothingPreset::SuitCasual),
        ("suit_formal",      ClothingPreset::SuitFormal),
        ("uniform_school",   ClothingPreset::UniformSchool),
        ("uniform_military", ClothingPreset::UniformMilitary),
    ];

    let mut ocount = 0;
    for (preset_name, preset) in &outfit_presets {
        for (color_name, color, secondary) in &outfit_colors {
            let name = format!("outfit_{preset_name}_{color_name}");
            let params = ClothingParams {
                preset: *preset,
                color: *color,
                secondary_color: *secondary,
                fit: 0.5,
            };
            let part = body::generate_clothing(&params, &body_params);
            let mesh = CharacterMesh { parts: vec![part], skeleton: None, blendshape_targets: vec![] };
            let def = CharacterDef { clothing: params, ..Default::default() };
            let glb = export::export_glb(&mesh, &def);
            let path = format!("{out_dir}/{name}.glb");
            std::fs::write(&path, &glb).unwrap();
            ocount += 1;
        }
    }
    println!("Outfit: {ocount} files");
    println!("Total: {} files in {out_dir}/", count + ocount);
}
