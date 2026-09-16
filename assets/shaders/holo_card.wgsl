// Contour holographique des dés : contour pulsant, balayage irisé, dos neutre
// pour un dé masqué (TASK-91). Un Material2d rendu en MainPass sur le quad de
// chaque dé, jamais un post-process.
//
// Le document d'Étape 7 ne donne aucun corps pour ce shader ; ce qui est
// normatif, et tenu ici : la struct miroir du bloc Rust de `graphics/holo.rs`,
// cinq champs aux offsets 0 / 16 / 20 / 24 / 28, trente-deux octets ; le
// temps lu sur l'horloge du GPU, aucun champ de temps dans l'uniforme ; le
// masquage fait ici, la face n'étant pas échantillonnée du tout sous le
// masque ; uniformes et échantillonnage seuls, pour webgl2 comme WebGPU.
//
// Bindings : le bind group du matériau 2D, passé par Bevy comme def
// MATERIAL_BIND_GROUP (bevy_sprite_render-0.19.1/src/mesh2d/material.rs:470),
// et les attributs du matériau de TASK-90 : uniforme en 0, `#[texture(1)]`,
// `#[sampler(2)]`. Les imports sont ceux du vortex (TASK-85), confirmés dans
// les sources.
//
// Le dessin, justifié : le contour est la distance au bord du quad,
// `outline_width` en centièmes du quad ; la pulsation vaut 0,75 + 0,25·sin(4t),
// soit 0,64 Hz, sous le plafond de trois changements de luminance par seconde
// de l'Étape 4 ; le balayage irisé est une teinte défilant en diagonale, mise
// à l'échelle par `rainbow_shift`, donc nulle pour les variantes unies ; sous
// le masque, un gris neutre remplace la face, et la valeur du dé n'entre dans
// aucun calcul.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_sprite::mesh2d_view_bindings::globals

struct HoloUniform {
    outline_color: vec4<f32>,   // offset  0
    outline_width: f32,         // offset 16
    rainbow_shift: f32,         // offset 20
    mask_face:     f32,         // offset 24
    _pad:          f32,         // offset 28  -> taille 32
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> material: HoloUniform;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var face_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var face_sampler: sampler;

// Une teinte de l'arc-en-ciel, pour une phase dans [0, 1[.
fn rainbow(h: f32) -> vec3<f32> {
    let k = fract(vec3<f32>(h, h + 1.0 / 3.0, h + 2.0 / 3.0));
    return clamp(abs(k * 6.0 - 3.0) - 1.0, vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;

    // La face, ou le dos neutre : sous le masque, aucun échantillonnage.
    var base: vec3<f32>;
    if (material.mask_face > 0.5) {
        base = vec3<f32>(0.30, 0.30, 0.35);
    } else {
        base = textureSample(face_texture, face_sampler, uv).rgb;
    }

    // Balayage irisé en diagonale, animé par le GPU, nul pour l'uni.
    let sweep = fract((uv.x + uv.y) * 0.5 - globals.time * 0.25);
    let iridescence = rainbow(sweep) * material.rainbow_shift * 0.35;

    // Contour pulsant, à la distance au bord du quad.
    let edge = min(min(uv.x, 1.0 - uv.x), min(uv.y, 1.0 - uv.y));
    let width = material.outline_width * 0.02;
    let ring = 1.0 - smoothstep(width * 0.6, width, edge);
    let pulse = 0.75 + 0.25 * sin(globals.time * 4.0);

    let col = mix(base + iridescence, material.outline_color.rgb * pulse, ring * material.outline_color.a);
    return vec4<f32>(col, 1.0);
}
