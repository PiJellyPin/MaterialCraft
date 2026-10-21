//! The material templates shipped with MaterialCraft: stylized PBR materials built from nodes, each a complete `.sbs`
//! package with its six PBR outputs, plus a small preview of its base colour.

/// `(identifier, package text, preview PNG)` of every shipped material template.
pub const MATERIALS: &[(&str, &str, &[u8])] = &[
    ("bark", include_str!("../templates/bark.sbs"), include_bytes!("../templates/previews/bark.png")),
    ("brick_wall", include_str!("../templates/brick_wall.sbs"), include_bytes!("../templates/previews/brick_wall.png")),
    ("castle_wall", include_str!("../templates/castle_wall.sbs"), include_bytes!("../templates/previews/castle_wall.png")),
    ("ceramic_tiles", include_str!("../templates/ceramic_tiles.sbs"), include_bytes!("../templates/previews/ceramic_tiles.png")),
    ("cobblestone", include_str!("../templates/cobblestone.sbs"), include_bytes!("../templates/previews/cobblestone.png")),
    ("cracked_desert", include_str!("../templates/cracked_desert.sbs"), include_bytes!("../templates/previews/cracked_desert.png")),
    ("crystal", include_str!("../templates/crystal.sbs"), include_bytes!("../templates/previews/crystal.png")),
    ("dirt_path", include_str!("../templates/dirt_path.sbs"), include_bytes!("../templates/previews/dirt_path.png")),
    ("gold", include_str!("../templates/gold.sbs"), include_bytes!("../templates/previews/gold.png")),
    ("grass", include_str!("../templates/grass.sbs"), include_bytes!("../templates/previews/grass.png")),
    ("hex_tiles", include_str!("../templates/hex_tiles.sbs"), include_bytes!("../templates/previews/hex_tiles.png")),
    ("ice", include_str!("../templates/ice.sbs"), include_bytes!("../templates/previews/ice.png")),
    ("lava_rock", include_str!("../templates/lava_rock.sbs"), include_bytes!("../templates/previews/lava_rock.png")),
    ("leather", include_str!("../templates/leather.sbs"), include_bytes!("../templates/previews/leather.png")),
    ("leaves", include_str!("../templates/leaves.sbs"), include_bytes!("../templates/previews/leaves.png")),
    ("log_end", include_str!("../templates/log_end.sbs"), include_bytes!("../templates/previews/log_end.png")),
    ("magic_runes", include_str!("../templates/magic_runes.sbs"), include_bytes!("../templates/previews/magic_runes.png")),
    ("marble", include_str!("../templates/marble.sbs"), include_bytes!("../templates/previews/marble.png")),
    ("metal_grate", include_str!("../templates/metal_grate.sbs"), include_bytes!("../templates/previews/metal_grate.png")),
    ("mossy_rock", include_str!("../templates/mossy_rock.sbs"), include_bytes!("../templates/previews/mossy_rock.png")),
    ("mud", include_str!("../templates/mud.sbs"), include_bytes!("../templates/previews/mud.png")),
    ("painted_metal", include_str!("../templates/painted_metal.sbs"), include_bytes!("../templates/previews/painted_metal.png")),
    ("parquet", include_str!("../templates/parquet.sbs"), include_bytes!("../templates/previews/parquet.png")),
    ("plastic", include_str!("../templates/plastic.sbs"), include_bytes!("../templates/previews/plastic.png")),
    ("riveted_steel", include_str!("../templates/riveted_steel.sbs"), include_bytes!("../templates/previews/riveted_steel.png")),
    ("rusty_metal", include_str!("../templates/rusty_metal.sbs"), include_bytes!("../templates/previews/rusty_metal.png")),
    ("sand", include_str!("../templates/sand.sbs"), include_bytes!("../templates/previews/sand.png")),
    ("sandstone", include_str!("../templates/sandstone.sbs"), include_bytes!("../templates/previews/sandstone.png")),
    ("scifi_panel", include_str!("../templates/scifi_panel.sbs"), include_bytes!("../templates/previews/scifi_panel.png")),
    ("slime", include_str!("../templates/slime.sbs"), include_bytes!("../templates/previews/slime.png")),
    ("snow", include_str!("../templates/snow.sbs"), include_bytes!("../templates/previews/snow.png")),
    ("stone_tiles", include_str!("../templates/stone_tiles.sbs"), include_bytes!("../templates/previews/stone_tiles.png")),
    ("wicker", include_str!("../templates/wicker.sbs"), include_bytes!("../templates/previews/wicker.png")),
    ("wood_planks", include_str!("../templates/wood_planks.sbs"), include_bytes!("../templates/previews/wood_planks.png")),
    ("woven_cloth", include_str!("../templates/woven_cloth.sbs"), include_bytes!("../templates/previews/woven_cloth.png")),
];

/// The name as shown in the launcher: "mossy_rock" -> "Mossy Rock".
pub fn title(ident: &str) -> String {
    ident.split('_').map(|w| { let mut c = w.chars(); c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default() }).collect::<Vec<_>>().join(" ")
}
