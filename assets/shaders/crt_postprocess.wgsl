// Filtre cathodique plein écran, set PostProcess, après le tonemapping.
//
// Forme minimale chargeable, posée par TASK-82 : un point d'entrée `fragment`
// sans entrée, qui rend une couleur opaque. Ce corps ne lit pas l'image de
// l'écran : il ne doit être ordonnancé par personne avant que TASK-89 n'écrive
// le vrai filtre. TASK-88 pose le matériau et TASK-89 le corps.
//
// Aucune modulation temporelle ici, ni maintenant ni plus tard : les scanlines
// sont statiques, par le critère de photosensibilité de l'Étape 4.

@fragment
fn fragment() -> @location(0) vec4<f32> {
    // Noir franc : « hors du tube », la couleur que le filtre rend hors cadre.
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}
