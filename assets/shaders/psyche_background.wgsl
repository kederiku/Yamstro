// Vortex d'arrière-plan, rendu en MainPass sur le quad de fond (TASK-85).
//
// Verbatim du § 3.1 et du § 3(b) du document d'Étape 7, sur trois points
// confirmés dans les sources 0.19.1 : les chemins d'import gardent leur
// `#define_import_path bevy_sprite::…` malgré la crate `bevy_sprite_render`
// (bevy_sprite_render-0.19.1/src/mesh2d/*.wgsl, ligne 1) ; le bind group du
// matériau 2D est l'index 2 (mesh2d/material.rs:60), que Bevy passe au shader
// comme def MATERIAL_BIND_GROUP (material.rs:470), l'écriture de son propre
// ColorMaterial ; le champ de viewport est `view.viewport: vec4<f32>`, soit
// (x, y, largeur, hauteur) (bevy_render-0.19.1/src/view/view.wgsl:58).
//
// La struct est le miroir exact du bloc Rust de `graphics/background.rs`,
// champ pour champ, offsets en commentaire : aucun test de taille ne voit un
// ordre différent, seule cette relecture le tient, et la CI l'exige des deux
// côtés. Le temps vient du GPU par `globals.time` : aucun champ de temps dans
// l'uniforme, aucun système n'écrit ce matériau hors transition de palette.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_sprite::mesh2d_view_bindings::{globals, view}

struct BackgroundUniform {
    primary_color:   vec4<f32>,   // offset  0
    secondary_color: vec4<f32>,   // offset 16
    accent_color:    vec4<f32>,   // offset 32
    speed:           f32,         // offset 48
    swirl_factor:    f32,         // offset 52
    _pad:            vec2<f32>,   // offset 56  -> taille 64
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> material: BackgroundUniform;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let t      = globals.time * material.speed;
    let aspect = view.viewport.z / view.viewport.w;   // pas de déformation au resize
    var uv     = in.uv * 2.0 - 1.0;
    uv.x      *= aspect;

    let r      = length(uv);
    let a      = atan2(uv.y, uv.x) + r * material.swirl_factor + t * 0.35;
    let spiral = sin(a * 3.0 + sin(r * 6.0 - t) * 1.7);

    var col = mix(material.primary_color.rgb, material.secondary_color.rgb,
                  0.5 + 0.5 * spiral);
    col = mix(col, material.accent_color.rgb,
              pow(clamp(1.0 - r, 0.0, 1.0), 3.0));
    return vec4<f32>(col, 1.0);
}
