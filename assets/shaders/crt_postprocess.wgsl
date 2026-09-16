// Filtre cathodique plein écran : courbure, scanlines statiques, aberration
// chromatique, vignettage (TASK-89). Set PostProcess de la 2D, après le
// tonemapping, ordonnancé par le matériau de TASK-88, et présent seulement
// quand le filtre est actif : l'identité de l'image tient à l'absence de
// passe, pas à un booléen ici.
//
// Verbatim du § 3.3 du document d'Étape 7. Les trois déclarations que le
// document demandait de relever, pas de deviner : le bind group 0 est fixé
// par le trait, texture d'écran en 0, sampler en 1, uniforme en 2
// (bevy_core_pipeline-0.19.1/src/fullscreen_material.rs:142-147), et c'est
// le groupe 0 que la passe lie avec l'index dynamique de l'uniforme (:337).
// L'import est celui des propres shaders de Bevy (blit.wgsl,
// tonemapping.wgsl) ; la struct a deux champs, position et uv
// (fullscreen_vertex_shader/fullscreen.wgsl:3-8).
//
// La struct est le miroir exact du bloc Rust de `graphics/crt.rs`, quatre
// f32 aux offsets 0 / 4 / 8 / 12, seize octets : un décalage ne produit
// aucune erreur, il produit des couleurs fausses, et la CI tient le miroir
// des deux côtés.
//
// Aucune modulation temporelle : les scanlines sont statiques, leur densité
// liée aux pixels de la cible. C'est le seul shader de l'étape qui n'importe
// pas l'horloge du GPU, par le critère de photosensibilité de l'Étape 4, et
// c'est délibéré. Hors du tube, noir franc, avant tout échantillonnage : sans
// ce retour, le sampler étirerait le dernier texel en traînées sur les bords.
// Uniquement des uniformes et de l'échantillonnage, ni storage, ni compute,
// ni écriture de texture, pour webgl2 comme pour WebGPU.

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct CrtUniform {
    curvature:            f32,   // offset  0
    scanline_intensity:   f32,   // offset  4
    vignette_roundness:   f32,   // offset  8
    chromatic_aberration: f32,   // offset 12  -> taille 16
};

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@group(0) @binding(2) var<uniform> settings: CrtUniform;

fn barrel(uv: vec2<f32>, k: f32) -> vec2<f32> {
    let c = uv * 2.0 - 1.0;
    return (c * (1.0 + k * dot(c, c))) * 0.5 + 0.5;
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let uv = barrel(in.uv, settings.curvature);
    // hors du tube : noir franc, jamais un bord étiré par le sampler
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    let off = (uv - 0.5) * settings.chromatic_aberration;      // aberration radiale
    var col = vec3<f32>(
        textureSample(screen_texture, screen_sampler, uv + off).r,
        textureSample(screen_texture, screen_sampler, uv).g,
        textureSample(screen_texture, screen_sampler, uv - off).b,
    );

    // scanlines STATIQUES, densité liée aux pixels de la cible, aucun scintillement
    let h = f32(textureDimensions(screen_texture).y);
    let s = sin(uv.y * h * 3.14159265);
    col = col * (1.0 - settings.scanline_intensity * 0.5 * (1.0 - s * s));

    let vig = 1.0 - smoothstep(settings.vignette_roundness, 0.85,
                               distance(uv, vec2<f32>(0.5, 0.5)));
    return vec4<f32>(col * vig, 1.0);
}
