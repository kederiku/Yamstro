// Contour pulsant et balayage irisé des dés et des cartes de relique, MainPass.
//
// Forme minimale chargeable, posée par TASK-82 : un point d'entrée `fragment`
// sans entrée, qui rend une couleur opaque. TASK-90 pose le matériau et sa
// banque de variantes, TASK-91 écrit le corps et l'échange de poignée sur les
// marqueurs Scoring et Hidden.

@fragment
fn fragment() -> @location(0) vec4<f32> {
    // Gris neutre opaque : le dos de dé que la variante masquée dessinera.
    return vec4<f32>(0.30, 0.30, 0.35, 1.0);
}
