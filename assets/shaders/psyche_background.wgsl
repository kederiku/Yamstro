// Vortex d'arrière-plan, rendu en MainPass sur le quad de fond.
//
// Forme minimale chargeable, posée par TASK-82 : un point d'entrée `fragment`
// sans entrée, qui rend une couleur opaque. Un fragment sans entrée est valide
// contre n'importe quel étage vertex, donc contre le pipeline 2D à venir.
// TASK-85 écrit le corps ; ce fichier n'importe rien pour ne figer aucun
// chemin que TASK-85 devra confirmer sur la documentation 0.19.1.

@fragment
fn fragment() -> @location(0) vec4<f32> {
    // Bleu nuit opaque : la dominante de la Petite Mise, palette par défaut.
    return vec4<f32>(0.05, 0.05, 0.16, 1.0);
}
