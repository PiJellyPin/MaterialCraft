//! The material templates shipped with MaterialCraft: stylized PBR materials built from nodes, each a complete `.sbs`
//! package with its six PBR outputs, plus a small preview of its base colour.

/// `(identifier, package text, preview PNG)` of every shipped material template.
pub const MATERIALS: &[(&str, &str, &[u8])] = &[
    ("bark", include_str!("../assets/templates/bark.sbs"), include_bytes!("../assets/templates/previews/bark.png")),
    ("brick_wall", include_str!("../assets/templates/brick_wall.sbs"), include_bytes!("../assets/templates/previews/brick_wall.png")),
    ("castle_wall", include_str!("../assets/templates/castle_wall.sbs"), include_bytes!("../assets/templates/previews/castle_wall.png")),
    ("ceramic_tiles", include_str!("../assets/templates/ceramic_tiles.sbs"), include_bytes!("../assets/templates/previews/ceramic_tiles.png")),
    ("cobblestone", include_str!("../assets/templates/cobblestone.sbs"), include_bytes!("../assets/templates/previews/cobblestone.png")),
    ("cracked_desert", include_str!("../assets/templates/cracked_desert.sbs"), include_bytes!("../assets/templates/previews/cracked_desert.png")),
    ("crystal", include_str!("../assets/templates/crystal.sbs"), include_bytes!("../assets/templates/previews/crystal.png")),
    ("dirt_path", include_str!("../assets/templates/dirt_path.sbs"), include_bytes!("../assets/templates/previews/dirt_path.png")),
    ("gold", include_str!("../assets/templates/gold.sbs"), include_bytes!("../assets/templates/previews/gold.png")),
    ("grass", include_str!("../assets/templates/grass.sbs"), include_bytes!("../assets/templates/previews/grass.png")),
    ("hex_tiles", include_str!("../assets/templates/hex_tiles.sbs"), include_bytes!("../assets/templates/previews/hex_tiles.png")),
    ("ice", include_str!("../assets/templates/ice.sbs"), include_bytes!("../assets/templates/previews/ice.png")),
    ("lava_rock", include_str!("../assets/templates/lava_rock.sbs"), include_bytes!("../assets/templates/previews/lava_rock.png")),
    ("leather", include_str!("../assets/templates/leather.sbs"), include_bytes!("../assets/templates/previews/leather.png")),
    ("leaves", include_str!("../assets/templates/leaves.sbs"), include_bytes!("../assets/templates/previews/leaves.png")),
    ("log_end", include_str!("../assets/templates/log_end.sbs"), include_bytes!("../assets/templates/previews/log_end.png")),
    ("magic_runes", include_str!("../assets/templates/magic_runes.sbs"), include_bytes!("../assets/templates/previews/magic_runes.png")),
    ("marble", include_str!("../assets/templates/marble.sbs"), include_bytes!("../assets/templates/previews/marble.png")),
    ("metal_grate", include_str!("../assets/templates/metal_grate.sbs"), include_bytes!("../assets/templates/previews/metal_grate.png")),
    ("mossy_rock", include_str!("../assets/templates/mossy_rock.sbs"), include_bytes!("../assets/templates/previews/mossy_rock.png")),
    ("mud", include_str!("../assets/templates/mud.sbs"), include_bytes!("../assets/templates/previews/mud.png")),
    ("painted_metal", include_str!("../assets/templates/painted_metal.sbs"), include_bytes!("../assets/templates/previews/painted_metal.png")),
    ("parquet", include_str!("../assets/templates/parquet.sbs"), include_bytes!("../assets/templates/previews/parquet.png")),
    ("plastic", include_str!("../assets/templates/plastic.sbs"), include_bytes!("../assets/templates/previews/plastic.png")),
    ("riveted_steel", include_str!("../assets/templates/riveted_steel.sbs"), include_bytes!("../assets/templates/previews/riveted_steel.png")),
    ("rusty_metal", include_str!("../assets/templates/rusty_metal.sbs"), include_bytes!("../assets/templates/previews/rusty_metal.png")),
    ("sand", include_str!("../assets/templates/sand.sbs"), include_bytes!("../assets/templates/previews/sand.png")),
    ("sandstone", include_str!("../assets/templates/sandstone.sbs"), include_bytes!("../assets/templates/previews/sandstone.png")),
    ("scifi_panel", include_str!("../assets/templates/scifi_panel.sbs"), include_bytes!("../assets/templates/previews/scifi_panel.png")),
    ("slime", include_str!("../assets/templates/slime.sbs"), include_bytes!("../assets/templates/previews/slime.png")),
    ("snow", include_str!("../assets/templates/snow.sbs"), include_bytes!("../assets/templates/previews/snow.png")),
    ("stone_tiles", include_str!("../assets/templates/stone_tiles.sbs"), include_bytes!("../assets/templates/previews/stone_tiles.png")),
    ("wicker", include_str!("../assets/templates/wicker.sbs"), include_bytes!("../assets/templates/previews/wicker.png")),
    ("wood_planks", include_str!("../assets/templates/wood_planks.sbs"), include_bytes!("../assets/templates/previews/wood_planks.png")),
    ("woven_cloth", include_str!("../assets/templates/woven_cloth.sbs"), include_bytes!("../assets/templates/previews/woven_cloth.png")),
];

/// The name as shown in the launcher: "mossy_rock" -> "Mossy Rock".
pub fn title(ident: &str) -> String {
    ident.split('_').map(|w| { let mut c = w.chars(); c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default() }).collect::<Vec<_>>().join(" ")
}
