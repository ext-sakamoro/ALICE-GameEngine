//! VRM 0.x (`extensions.VRM`) read into the VRM 1.0 model of [`crate::vrm`]
//!
//! - **meta**: kept verbatim as [`Vrm0Meta`]. The 0.x licence fields do not translate to the
//!   1.0 ones, so none is invented ([`crate::vrm::Licence::Vrm0`]).
//! - **humanoid**: `humanBones[{bone, node}]`, with the 0.x thumb names moved to the 1.0 ones
//!   (`thumbProximal` → `thumbMetacarpal`, `thumbIntermediate` → `thumbProximal`).
//! - **expressions**: `blendShapeMaster.blendShapeGroups`. A preset name maps to its 1.0 name
//!   (`a` → `aa`, `joy` → `happy`, `blink_l` → `blinkLeft`, ...); `unknown` becomes a custom
//!   expression under the group's name. A bind names a mesh in 0.x and a node in 1.0: it goes to
//!   the first node that holds that mesh. Weights go from 0..100 to 0..1.
//! - **look-at**: `firstPerson`, its `xRange` as the input maximum and `yRange` as the output
//!   scale; the offset is from `firstPersonBone`.
//! - **materials**: `materialProperties[i]` of shader `VRM/MToon` gives material `i` its `MToon`
//!   parameters, converted as the reference migration does: colours from sRGB to linear, the
//!   shading range of `_ShadeShift` / `_ShadeToony` to the 1.0 shift and toony, the outline
//!   width from centimetres (world) or percent of half the screen height (screen), the lighting
//!   mix only in the mixed colour mode.
//! - **orientation**: a 0.x model faces −Z and a 1.0 model +Z. Everything is turned 180° about
//!   Y as it is read (positions, normals, morph deltas, node translations and rotations, inverse
//!   bind matrices), so every reader of the model sees the 1.0 convention. The look-at offset is
//!   the exception: 0.x stores it in the left-handed frame of the exporting engine, where the
//!   model already faces +Z, so only its x changes sign.
//!
//! References: VRM 0.x specification (`extensions.VRM`), `MToon` 0.x shader properties, and the
//! reference implementation's 0.x → 1.0 material migration.

// no fused multiply-add: a file reads to the same bits on every target
#![allow(clippy::suboptimal_flops)]
// the 0.x modes are small integers written as floats: compared exactly
#![allow(clippy::float_cmp)]

use serde_json::Value;

use crate::vrm::{
    floats_n, index, path_str, LookAt, LookAtKind, MToon, Material, MeshPrimitive, MorphBind, Node,
    OutlineWidthMode, RangeMap, Skin, VrmError, VrmExpression,
};

/// The VRM 0.x meta, every field as the file states it (absent or empty strings are `None`)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vrm0Meta {
    /// `title`
    pub title: Option<String>,
    /// `version`
    pub version: Option<String>,
    /// `author`
    pub author: Option<String>,
    /// `contactInformation`
    pub contact_information: Option<String>,
    /// `reference`
    pub reference: Option<String>,
    /// `allowedUserName`: `OnlyAuthor`, `ExplicitlyLicensedPerson` or `Everyone`
    pub allowed_user_name: Option<String>,
    /// `violentUssageName`: `Disallow` or `Allow`
    pub violent_usage_name: Option<String>,
    /// `sexualUssageName`: `Disallow` or `Allow`
    pub sexual_usage_name: Option<String>,
    /// `commercialUssageName`: `Disallow` or `Allow`
    pub commercial_usage_name: Option<String>,
    /// `otherPermissionUrl`
    pub other_permission_url: Option<String>,
    /// `licenseName`: `Redistribution_Prohibited`, `CC0`, `CC_BY`, `CC_BY_NC`, `CC_BY_SA`,
    /// `CC_BY_NC_SA`, `CC_BY_ND`, `CC_BY_NC_ND` or `Other`
    pub license_name: Option<String>,
    /// `otherLicenseUrl`
    pub other_license_url: Option<String>,
}

fn text(m: &Value, key: &str) -> Result<Option<String>, VrmError> {
    Ok(path_str(m, key, "meta")?.filter(|s| !s.is_empty()))
}

fn one_of(m: &Value, key: &str, allowed: &[&str]) -> Result<Option<String>, VrmError> {
    match text(m, key)? {
        Some(s) if !allowed.contains(&s.as_str()) => Err(VrmError::BadValue(format!("meta.{key}"))),
        s => Ok(s),
    }
}

fn read_meta(m: &Value) -> Result<Vrm0Meta, VrmError> {
    let yes_no = ["Disallow", "Allow"];
    Ok(Vrm0Meta {
        title: text(m, "title")?,
        version: text(m, "version")?,
        author: text(m, "author")?,
        contact_information: text(m, "contactInformation")?,
        reference: text(m, "reference")?,
        allowed_user_name: one_of(
            m,
            "allowedUserName",
            &["OnlyAuthor", "ExplicitlyLicensedPerson", "Everyone"],
        )?,
        violent_usage_name: one_of(m, "violentUssageName", &yes_no)?,
        sexual_usage_name: one_of(m, "sexualUssageName", &yes_no)?,
        commercial_usage_name: one_of(m, "commercialUssageName", &yes_no)?,
        other_permission_url: text(m, "otherPermissionUrl")?,
        license_name: one_of(
            m,
            "licenseName",
            &[
                "Redistribution_Prohibited",
                "CC0",
                "CC_BY",
                "CC_BY_NC",
                "CC_BY_SA",
                "CC_BY_NC_SA",
                "CC_BY_ND",
                "CC_BY_NC_ND",
                "Other",
            ],
        )?,
        other_license_url: text(m, "otherLicenseUrl")?,
    })
}

/// The 1.0 name of a 0.x humanoid bone
fn bone_name(name: &str) -> String {
    for side in ["left", "right"] {
        if let Some(rest) = name.strip_prefix(side) {
            match rest {
                "ThumbProximal" => return format!("{side}ThumbMetacarpal"),
                "ThumbIntermediate" => return format!("{side}ThumbProximal"),
                _ => {}
            }
        }
    }
    name.to_owned()
}

/// The 1.0 preset name of a 0.x preset (`None` for `unknown` and any name 0.x does not define)
fn preset_name(name: &str) -> Option<&'static str> {
    Some(match name.to_ascii_lowercase().as_str() {
        "neutral" => "neutral",
        "a" => "aa",
        "i" => "ih",
        "u" => "ou",
        "e" => "ee",
        "o" => "oh",
        "blink" => "blink",
        "blink_l" => "blinkLeft",
        "blink_r" => "blinkRight",
        "joy" => "happy",
        "angry" => "angry",
        "sorrow" => "sad",
        "fun" => "relaxed",
        "lookup" => "lookUp",
        "lookdown" => "lookDown",
        "lookleft" => "lookLeft",
        "lookright" => "lookRight",
        _ => return None,
    })
}

#[allow(clippy::cast_possible_truncation)]
fn number(v: &Value, key: &str, path: &str) -> Result<Option<f32>, VrmError> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(x) => x
            .as_f64()
            .map(|x| x as f32)
            .filter(|x| x.is_finite())
            .map(Some)
            .ok_or_else(|| VrmError::BadValue(format!("{path}.{key}"))),
    }
}

fn read_expressions(vrm: &Value, json: &Value) -> Result<Vec<VrmExpression>, VrmError> {
    let Some(groups) = vrm
        .get("blendShapeMaster")
        .and_then(|b| b.get("blendShapeGroups"))
    else {
        return Ok(Vec::new());
    };
    let groups = groups
        .as_array()
        .ok_or_else(|| VrmError::BadValue("blendShapeMaster.blendShapeGroups".into()))?;
    let nodes = json.get("nodes").and_then(Value::as_array);
    let mut presets = Vec::new();
    let mut custom = Vec::new();
    for (g, group) in groups.iter().enumerate() {
        let path = format!("blendShapeMaster.blendShapeGroups[{g}]");
        let name = path_str(group, "name", &path)?.unwrap_or_default();
        let preset = path_str(group, "presetName", &path)?
            .as_deref()
            .and_then(preset_name);
        let mut binds = Vec::new();
        if let Some(arr) = group.get("binds") {
            let arr = arr
                .as_array()
                .ok_or_else(|| VrmError::BadValue(format!("{path}.binds")))?;
            for b in arr {
                let bp = format!("{path}.binds");
                let mesh = index(b, "mesh", &bp)?;
                // the first node that holds the mesh
                let node = nodes
                    .and_then(|n| {
                        n.iter().position(|n| {
                            n.get("mesh").and_then(Value::as_u64) == Some(u64::from(mesh))
                        })
                    })
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or_else(|| VrmError::OutOfRange(format!("{bp}.mesh")))?;
                let weight = number(b, "weight", &bp)?
                    .ok_or_else(|| VrmError::Missing(format!("{bp}.weight")))?;
                binds.extend([MorphBind {
                    node,
                    index: index(b, "index", &bp)?,
                    weight: weight / 100.0,
                }]);
            }
        }
        let is_binary = match group.get("isBinary") {
            None => false,
            Some(Value::Bool(b)) => *b,
            Some(_) => return Err(VrmError::BadValue(format!("{path}.isBinary"))),
        };
        let e = VrmExpression {
            name: preset.map_or(name, str::to_owned),
            preset: preset.is_some(),
            binds,
            is_binary,
        };
        if e.preset {
            presets.extend([e]);
        } else {
            custom.extend([e]);
        }
    }
    // as the 1.0 reader orders them: presets by name, then the custom ones by name
    presets.sort_by(|a, b| a.name.cmp(&b.name));
    custom.sort_by(|a, b| a.name.cmp(&b.name));
    presets.extend(custom);
    Ok(presets)
}

fn read_look_at(fp: &Value) -> Result<LookAt, VrmError> {
    let kind = match path_str(fp, "lookAtTypeName", "firstPerson")?.as_deref() {
        None | Some("Bone") => LookAtKind::Bone,
        Some("BlendShape") => LookAtKind::Expression,
        Some(_) => return Err(VrmError::BadValue("firstPerson.lookAtTypeName".into())),
    };
    let map = |key: &str| -> Result<RangeMap, VrmError> {
        let Some(m) = fp.get(key) else {
            return Ok(RangeMap::DEFAULT);
        };
        let path = format!("firstPerson.{key}");
        let range = |k: &str, d: f32| -> Result<f32, VrmError> {
            match number(m, k, &path)? {
                None => Ok(d),
                Some(x) if x >= 0.0 => Ok(x),
                Some(_) => Err(VrmError::BadValue(format!("{path}.{k}"))),
            }
        };
        Ok(RangeMap {
            input_max_value: range("xRange", RangeMap::DEFAULT.input_max_value)?,
            output_scale: range("yRange", RangeMap::DEFAULT.output_scale)?,
        })
    };
    let offset = match fp.get("firstPersonBoneOffset") {
        None => [0.0; 3],
        Some(o) => {
            let c = |k: &str| {
                number(o, k, "firstPerson.firstPersonBoneOffset").map(Option::unwrap_or_default)
            };
            [c("x")?, c("y")?, c("z")?]
        }
    };
    Ok(LookAt {
        kind,
        // stored in the left-handed frame of the exporting engine, where the model faces +Z:
        // only x changes sign to reach the 1.0 frame
        offset_from_head_bone: [-offset[0], offset[1], offset[2]],
        horizontal_inner: map("lookAtHorizontalInner")?,
        horizontal_outer: map("lookAtHorizontalOuter")?,
        vertical_down: map("lookAtVerticalDown")?,
        vertical_up: map("lookAtVerticalUp")?,
    })
}

/// The parts of a 1.0 document that a 0.x extension gives
pub(crate) struct Vrm0Document {
    pub(crate) meta: Vrm0Meta,
    pub(crate) bones: Vec<(String, u32)>,
    pub(crate) expressions: Vec<VrmExpression>,
    pub(crate) look_at: Option<LookAt>,
    pub(crate) spec_version: String,
}

/// Reads the `extensions.VRM` object of a glTF JSON document
pub(crate) fn read_document(json: &Value, vrm: &Value) -> Result<Vrm0Document, VrmError> {
    let meta = read_meta(
        vrm.get("meta")
            .ok_or_else(|| VrmError::Missing("meta".into()))?,
    )?;
    let mut bones = Vec::new();
    if let Some(hb) = vrm.get("humanoid").and_then(|h| h.get("humanBones")) {
        let arr = hb
            .as_array()
            .ok_or_else(|| VrmError::BadValue("humanoid.humanBones".into()))?;
        for (k, b) in arr.iter().enumerate() {
            let path = format!("humanoid.humanBones[{k}]");
            let name = path_str(b, "bone", &path)?
                .ok_or_else(|| VrmError::Missing(format!("{path}.bone")))?;
            bones.extend([(bone_name(&name), index(b, "node", &path)?)]);
        }
        bones.sort();
    }
    Ok(Vrm0Document {
        meta,
        bones,
        expressions: read_expressions(vrm, json)?,
        look_at: vrm.get("firstPerson").map(read_look_at).transpose()?,
        spec_version: path_str(vrm, "specVersion", "VRM")?.unwrap_or_else(|| "0.0".into()),
    })
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// The `MToon` 1.0 parameters of a 0.x `materialProperties` entry of shader `VRM/MToon`, and its
/// lit colour (linear, alpha kept)
fn mtoon(p: &Value, path: &str) -> Result<([f32; 4], MToon), VrmError> {
    let fpath = format!("{path}.floatProperties");
    let vpath = format!("{path}.vectorProperties");
    let floats = p.get("floatProperties").unwrap_or(&Value::Null);
    let vectors = p.get("vectorProperties").unwrap_or(&Value::Null);
    let f = |k: &str, d: f32| number(floats, k, &fpath).map(|x| x.unwrap_or(d));
    let colour = |k: &str, d: [f32; 4]| floats_n(vectors, k, d, &vpath);
    let lit = colour("_Color", [1.0; 4])?;
    let shade = colour("_ShadeColor", [0.97, 0.81, 0.86, 1.0])?;
    let outline = colour("_OutlineColor", [0.0, 0.0, 0.0, 1.0])?;
    let rgb = |c: [f32; 4]| {
        [
            srgb_to_linear(c[0]),
            srgb_to_linear(c[1]),
            srgb_to_linear(c[2]),
        ]
    };
    // the 0.x shading range: from the shift to lerp(1, shift, toony)
    let (shift0, toony0) = (f("_ShadeShift", 0.0)?, f("_ShadeToony", 0.9)?);
    let lo = shift0;
    let hi = 1.0 + (shift0 - 1.0) * toony0;
    let width = f("_OutlineWidth", 0.5)?;
    // the modes are written as floats: 0, 1 or 2
    let mode = f("_OutlineWidthMode", 0.0)?;
    let (outline_width_mode, outline_width) = if mode == 0.0 {
        (OutlineWidthMode::None, 0.0)
    } else if mode == 1.0 {
        (OutlineWidthMode::World, width * 0.01)
    } else if mode == 2.0 {
        (OutlineWidthMode::Screen, width * 0.01 * 0.5)
    } else {
        return Err(VrmError::BadValue(format!("{fpath}._OutlineWidthMode")));
    };
    if outline_width < 0.0 {
        return Err(VrmError::BadValue(format!("{fpath}._OutlineWidth")));
    }
    let colour_mode = f("_OutlineColorMode", 0.0)?;
    let outline_lighting_mix = if colour_mode == 0.0 {
        0.0
    } else if colour_mode == 1.0 {
        f("_OutlineLightingMix", 1.0)?.clamp(0.0, 1.0)
    } else {
        return Err(VrmError::BadValue(format!("{fpath}._OutlineColorMode")));
    };
    let lit_linear = rgb(lit);
    Ok((
        [lit_linear[0], lit_linear[1], lit_linear[2], lit[3]],
        MToon {
            shade_color: rgb(shade),
            shading_shift: (-((hi + lo) * 0.5)).clamp(-1.0, 1.0),
            shading_toony: ((2.0 - (hi - lo)) * 0.5).clamp(0.0, 1.0),
            outline_width_mode,
            outline_width,
            outline_color: rgb(outline),
            outline_lighting_mix,
        },
    ))
}

/// Gives the materials their 0.x `MToon` parameters (`materialProperties[i]` belongs to material
/// `i`; materials of other shaders keep what the glTF material says)
pub(crate) fn apply_materials(vrm: &Value, materials: &mut [Material]) -> Result<(), VrmError> {
    let Some(props) = vrm.get("materialProperties") else {
        return Ok(());
    };
    let props = props
        .as_array()
        .ok_or_else(|| VrmError::BadValue("materialProperties".into()))?;
    for (i, p) in props.iter().enumerate() {
        let path = format!("materialProperties[{i}]");
        if path_str(p, "shader", &path)?.as_deref() != Some("VRM/MToon") {
            continue;
        }
        let m = materials
            .get_mut(i)
            .ok_or_else(|| VrmError::OutOfRange(path.clone()))?;
        let (lit, toon) = mtoon(p, &path)?;
        m.base_color = lit;
        m.mtoon = Some(toon);
    }
    Ok(())
}

/// A point or a direction turned 180° about Y
const fn turn(p: [f32; 3]) -> [f32; 3] {
    [-p[0], p[1], -p[2]]
}

/// Turns the model 180° about Y, so that it faces +Z as a 1.0 model does
pub(crate) fn turn_around(
    meshes: &mut [Vec<MeshPrimitive>],
    skins: &mut [Skin],
    nodes: &mut [Node],
) {
    let all = |v: &mut Vec<[f32; 3]>| v.iter_mut().for_each(|p| *p = turn(*p));
    for p in meshes.iter_mut().flatten() {
        all(&mut p.positions);
        if let Some(n) = &mut p.normals {
            all(n);
        }
        p.targets.iter_mut().for_each(all);
        p.target_normals.iter_mut().flatten().for_each(all);
    }
    for n in nodes.iter_mut() {
        n.translation = turn(n.translation);
        // R q R⁻¹ for R = 180° about Y
        let [x, y, z, w] = n.rotation;
        n.rotation = [-x, y, -z, w];
    }
    // R M R⁻¹ for R = diag(-1, 1, -1, 1): element (row r, column c) takes the sign s_r s_c
    let s = [-1.0_f32, 1.0, -1.0, 1.0];
    for m in skins.iter_mut().flat_map(|s| &mut s.inverse_bind_matrices) {
        for (k, e) in m.iter_mut().enumerate() {
            *e *= s[k % 4] * s[k / 4];
        }
    }
}
