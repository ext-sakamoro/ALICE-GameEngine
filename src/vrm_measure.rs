//! A VRM model as a format-neutral rig, so that a body can be measured in the units of the
//! proportion laws (`alice_lol_humanoid::measure`)
//!
//! Every node is a bone. A node the humanoid maps takes the humanoid bone's name (`hips`,
//! `leftUpperLeg`, `leftEye`, ...) and its rest-pose world position as its bind position; other
//! nodes are unnamed and unbound, so only the humanoid map decides which bone is which part.
//! Every skinned mesh is a rig mesh: its primitives' vertices skinned in the rest pose, each
//! following the joint that weighs most on it. The model's frame is VRM 1.0's (y up, z forward,
//! a 0.x file already turned by the reader).

use alice_lol_humanoid::rig::{Rig, RigBone, RigMesh};

use crate::vrm::VrmModel;
use crate::vrm_pose::{PoseError, VrmPose};

fn wide(p: [f32; 3]) -> [f64; 3] {
    p.map(f64::from)
}

/// The rig of a VRM model in its rest pose
///
/// # Errors
///
/// [`PoseError`] when the node tree or a mesh cannot be posed
pub fn rig(model: &VrmModel) -> Result<Rig, PoseError> {
    let pose = VrmPose::new(model)?;
    let world = pose.world_matrices();
    let mut bones: Vec<RigBone> = vec![
        RigBone {
            name: String::new(),
            parent: None,
            bind: None,
        };
        model.nodes.len()
    ];
    for (i, n) in model.nodes.iter().enumerate() {
        for &c in &n.children {
            if let Some(b) = bones.get_mut(c as usize) {
                b.parent = Some(i);
            }
        }
    }
    for (name, node) in &model.document.bones {
        if let (Some(b), Some(w)) = (bones.get_mut(*node as usize), world.get(*node as usize)) {
            let t = w.0.w_axis;
            b.name.clone_from(name);
            b.bind = Some(wide([t.x, t.y, t.z]));
        }
    }
    let mut meshes = Vec::new();
    for node in &model.nodes {
        let (Some(mi), Some(si)) = (node.mesh, node.skin) else {
            continue;
        };
        let Some(skin) = model.skins.get(si as usize) else {
            continue;
        };
        let mut positions = Vec::new();
        let mut leads = Vec::new();
        for (k, prim) in model.meshes[mi as usize].iter().enumerate() {
            let posed = pose.posed_positions(mi as usize, k)?;
            positions.extend(posed.iter().map(|p| wide(*p)));
            // the joint with the largest weight; a vertex without joints or weights follows none
            let lead = |i: usize| -> Option<usize> {
                let (js, ws) = (
                    prim.joints.as_ref()?.get(i)?,
                    prim.weights.as_ref()?.get(i)?,
                );
                let mut best: Option<(f32, u16)> = None;
                for (j, w) in js.iter().zip(ws) {
                    if *w > 0.0 && best.is_none_or(|(bw, _)| *w > bw) {
                        best = Some((*w, *j));
                    }
                }
                best.and_then(|(_, j)| skin.joints.get(usize::from(j)))
                    .map(|&n| n as usize)
            };
            leads.extend((0..posed.len()).map(lead));
        }
        meshes.extend([RigMesh { positions, leads }]);
    }
    Ok(Rig { bones, meshes })
}
