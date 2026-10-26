//! VRM 1.0 reader: the GLB container, the `VRMC_vrm` extension (meta with every licence field,
//! humanoid bones, expressions with every morph target bind) and the mesh data a renderer
//! needs (positions, normals, joints, weights, indices, morph target deltas, skins, nodes).
//!
//! Every length and offset in the file is checked against the bytes before it is used, and no
//! allocation is sized by a number the file states without that check: a broken file is an
//! error, never a panic. VRM 0.x (`extensions.VRM`) is named as [`VrmError::Vrm0`], not read
//! as an empty model.
//!
//! References: glTF 2.0 (GLB container, accessors) and `VRMC_vrm` 1.0 (meta, humanoid,
//! expressions).

use serde_json::Value;

/// Why a VRM file was not read
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum VrmError {
    /// Fewer bytes than a header, a chunk or a stated length needs
    Truncated,
    /// The magic is not `glTF`
    NotGlb,
    /// A GLB container version other than 2
    UnsupportedVersion(u32),
    /// The first chunk is not JSON, a chunk length is not a multiple of 4, or the JSON is not UTF-8
    BadChunk,
    /// The JSON does not parse
    Json(String),
    /// A glTF file without the `VRMC_vrm` extension
    NotVrm,
    /// A VRM 0.x file (`extensions.VRM`), which this reader does not read
    Vrm0,
    /// A field the specification requires is absent (its JSON path)
    Missing(String),
    /// A field has a value of the wrong type or outside the specification's set (its JSON path)
    BadValue(String),
    /// An accessor, buffer view or index points outside the data
    OutOfRange(String),
}

impl std::fmt::Display for VrmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncated => f.write_str("the file ends before a length it states"),
            Self::NotGlb => f.write_str("not a GLB file (magic is not glTF)"),
            Self::UnsupportedVersion(v) => write!(f, "GLB container version {v}, expected 2"),
            Self::BadChunk => f.write_str("the GLB chunks are not JSON then BIN, 4-byte aligned"),
            Self::Json(e) => write!(f, "the JSON chunk does not parse: {e}"),
            Self::NotVrm => f.write_str("a glTF file without the VRMC_vrm extension"),
            Self::Vrm0 => f.write_str("a VRM 0.x file (extensions.VRM), not read"),
            Self::Missing(p) => write!(f, "{p} is required and absent"),
            Self::BadValue(p) => write!(f, "{p} has a value outside the specification"),
            Self::OutOfRange(p) => write!(f, "{p} points outside the data"),
        }
    }
}

impl std::error::Error for VrmError {}

/// The two chunks of a GLB file
#[derive(Debug, Clone, Copy)]
pub struct Glb<'a> {
    /// The JSON chunk (trailing padding spaces included)
    pub json: &'a str,
    /// The BIN chunk, when the file has one
    pub bin: Option<&'a [u8]>,
}

const GLB_MAGIC: u32 = 0x4654_6C67;
const CHUNK_JSON: u32 = 0x4E4F_534A;
const CHUNK_BIN: u32 = 0x004E_4942;

fn le32(data: &[u8], at: usize) -> Result<u32, VrmError> {
    let b = data.get(at..at + 4).ok_or(VrmError::Truncated)?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Splits a GLB file into its JSON and BIN chunks
///
/// # Errors
///
/// [`VrmError::Truncated`], [`VrmError::NotGlb`], [`VrmError::UnsupportedVersion`] or
/// [`VrmError::BadChunk`]
pub fn split_glb(data: &[u8]) -> Result<Glb<'_>, VrmError> {
    if data.len() < 12 {
        return Err(VrmError::Truncated);
    }
    if le32(data, 0)? != GLB_MAGIC {
        return Err(VrmError::NotGlb);
    }
    let version = le32(data, 4)?;
    if version != 2 {
        return Err(VrmError::UnsupportedVersion(version));
    }
    let total = le32(data, 8)? as usize;
    if total > data.len() || total < 20 {
        return Err(VrmError::Truncated);
    }
    let data = &data[..total];
    let json_len = le32(data, 12)? as usize;
    if le32(data, 16)? != CHUNK_JSON || !json_len.is_multiple_of(4) {
        return Err(VrmError::BadChunk);
    }
    let json_end = 20usize.checked_add(json_len).ok_or(VrmError::Truncated)?;
    let json_bytes = data.get(20..json_end).ok_or(VrmError::Truncated)?;
    let json = std::str::from_utf8(json_bytes).map_err(|_| VrmError::BadChunk)?;
    let bin = if json_end < total {
        let bin_len = le32(data, json_end)? as usize;
        if le32(data, json_end + 4)? != CHUNK_BIN || !bin_len.is_multiple_of(4) {
            return Err(VrmError::BadChunk);
        }
        let start = json_end + 8;
        let end = start.checked_add(bin_len).ok_or(VrmError::Truncated)?;
        Some(data.get(start..end).ok_or(VrmError::Truncated)?)
    } else {
        None
    };
    Ok(Glb { json, bin })
}

/// The `VRMC_vrm` meta: who made the model and the licence it is under (every 1.0 field)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VrmMeta {
    /// `name`
    pub name: String,
    /// `version`
    pub version: Option<String>,
    /// `authors`, every one in order
    pub authors: Vec<String>,
    /// `copyrightInformation`
    pub copyright_information: Option<String>,
    /// `contactInformation`
    pub contact_information: Option<String>,
    /// `references`
    pub references: Vec<String>,
    /// `thirdPartyLicenses`
    pub third_party_licenses: Option<String>,
    /// `licenseUrl` (required)
    pub license_url: String,
    /// `avatarPermission`: `onlyAuthor` (default), `onlySeparatelyLicensedPerson` or `everyone`
    pub avatar_permission: String,
    /// `allowExcessivelyViolentUsage` (default false)
    pub allow_excessively_violent_usage: bool,
    /// `allowExcessivelySexualUsage` (default false)
    pub allow_excessively_sexual_usage: bool,
    /// `commercialUsage`: `personalNonProfit` (default), `personalProfit` or `corporation`
    pub commercial_usage: String,
    /// `allowPoliticalOrReligiousUsage` (default false)
    pub allow_political_or_religious_usage: bool,
    /// `allowAntisocialOrHateUsage` (default false)
    pub allow_antisocial_or_hate_usage: bool,
    /// `creditNotation`: `required` (default) or `unnecessary`
    pub credit_notation: String,
    /// `allowRedistribution` (default false)
    pub allow_redistribution: bool,
    /// `modification`: `prohibited` (default), `allowModification` or `allowModificationRedistribution`
    pub modification: String,
    /// `otherLicenseUrl`
    pub other_license_url: Option<String>,
}

/// One morph target bind of an expression
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MorphBind {
    /// The node whose mesh has the target
    pub node: u32,
    /// The morph target index in that mesh
    pub index: u32,
    /// The weight the target takes at full expression
    pub weight: f32,
}

/// One expression: a preset (`happy`, `aa`, `blink`, ...) or a custom one
#[derive(Debug, Clone, PartialEq)]
pub struct VrmExpression {
    /// Its name (the preset name, or the custom key)
    pub name: String,
    /// Whether it is under `expressions.preset`
    pub preset: bool,
    /// Every morph target bind, in file order
    pub binds: Vec<MorphBind>,
    /// `isBinary`
    pub is_binary: bool,
}

/// How a model looks at a target: by turning its eye bones, or by its look expressions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookAtKind {
    /// `bone`: the `leftEye` / `rightEye` bones turn; a range map's output is degrees
    Bone,
    /// `expression`: `lookLeft` / `lookRight` / `lookUp` / `lookDown` take weights; a range
    /// map's output is a weight
    Expression,
}

/// A look-at range map: an input angle up to `input_max_value` degrees maps linearly to
/// `output_scale` (degrees for the bone type, a weight for the expression type). The
/// specification gives no default for an absent map; this reader takes 90 and 10 for both
/// types, as the reference implementation does (so an expression weight, clamped to 1,
/// saturates at 9 degrees)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RangeMap {
    /// `inputMaxValue` (degrees)
    pub input_max_value: f32,
    /// `outputScale`
    pub output_scale: f32,
}

impl RangeMap {
    /// The specification's default map
    pub const DEFAULT: Self = Self {
        input_max_value: 90.0,
        output_scale: 10.0,
    };

    /// The output for an input angle (its magnitude is used; past the input maximum the
    /// output is the scale). With an input maximum of 0 the output is 0 for an input of 0 and
    /// the scale otherwise (the specification's recommendation, which avoids dividing by 0)
    #[must_use]
    pub fn map(self, angle_deg: f32) -> f32 {
        let a = angle_deg.abs();
        if self.input_max_value <= 0.0 {
            return if a == 0.0 { 0.0 } else { self.output_scale };
        }
        a.min(self.input_max_value) / self.input_max_value * self.output_scale
    }
}

/// The `VRMC_vrm` `lookAt`
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LookAt {
    /// `type`
    pub kind: LookAtKind,
    /// `offsetFromHeadBone` (the eyes' position from the head bone)
    pub offset_from_head_bone: [f32; 3],
    /// `rangeMapHorizontalInner`
    pub horizontal_inner: RangeMap,
    /// `rangeMapHorizontalOuter`
    pub horizontal_outer: RangeMap,
    /// `rangeMapVerticalDown`
    pub vertical_down: RangeMap,
    /// `rangeMapVerticalUp`
    pub vertical_up: RangeMap,
}

/// The `VRMC_vrm` extension
#[derive(Debug, Clone, PartialEq)]
pub struct VrmDocument {
    /// `specVersion`
    pub spec_version: String,
    /// The meta
    pub meta: VrmMeta,
    /// `(bone name, node index)`, in the order of the bone names
    pub bones: Vec<(String, u32)>,
    /// The presets in the order of their names, then the custom ones in the order of theirs
    pub expressions: Vec<VrmExpression>,
    /// `lookAt`, when the model has one
    pub look_at: Option<LookAt>,
}

fn read_range_map(v: &Value, key: &str) -> Result<RangeMap, VrmError> {
    let Some(m) = v.get(key) else {
        return Ok(RangeMap::DEFAULT);
    };
    let num = |k: &str, d: f32| -> Result<f32, VrmError> {
        match m.get(k) {
            None => Ok(d),
            #[allow(clippy::cast_possible_truncation)]
            Some(x) => x
                .as_f64()
                .map(|x| x as f32)
                .filter(|x| x.is_finite() && *x >= 0.0)
                .ok_or_else(|| VrmError::BadValue(format!("lookAt.{key}.{k}"))),
        }
    };
    Ok(RangeMap {
        input_max_value: num("inputMaxValue", RangeMap::DEFAULT.input_max_value)?,
        output_scale: num("outputScale", RangeMap::DEFAULT.output_scale)?,
    })
}

fn read_look_at(v: &Value) -> Result<LookAt, VrmError> {
    let kind = match path_str(v, "type", "lookAt")?.as_deref() {
        None | Some("bone") => LookAtKind::Bone,
        Some("expression") => LookAtKind::Expression,
        Some(_) => return Err(VrmError::BadValue("lookAt.type".into())),
    };
    Ok(LookAt {
        kind,
        offset_from_head_bone: floats_n(v, "offsetFromHeadBone", [0.0; 3], "lookAt")?,
        horizontal_inner: read_range_map(v, "rangeMapHorizontalInner")?,
        horizontal_outer: read_range_map(v, "rangeMapHorizontalOuter")?,
        vertical_down: read_range_map(v, "rangeMapVerticalDown")?,
        vertical_up: read_range_map(v, "rangeMapVerticalUp")?,
    })
}

fn path_str(v: &Value, key: &str, path: &str) -> Result<Option<String>, VrmError> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(VrmError::BadValue(format!("{path}.{key}"))),
    }
}

fn path_bool(v: &Value, key: &str, path: &str) -> Result<bool, VrmError> {
    match v.get(key) {
        None => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err(VrmError::BadValue(format!("{path}.{key}"))),
    }
}

fn path_enum(v: &Value, key: &str, path: &str, allowed: &[&str]) -> Result<String, VrmError> {
    match path_str(v, key, path)? {
        None => Ok(allowed[0].to_owned()),
        Some(s) if allowed.contains(&s.as_str()) => Ok(s),
        Some(_) => Err(VrmError::BadValue(format!("{path}.{key}"))),
    }
}

fn path_strings(v: &Value, key: &str, path: &str) -> Result<Vec<String>, VrmError> {
    match v.get(key) {
        None => Ok(Vec::new()),
        Some(Value::Array(a)) => a
            .iter()
            .map(|x| {
                x.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| VrmError::BadValue(format!("{path}.{key}")))
            })
            .collect(),
        Some(_) => Err(VrmError::BadValue(format!("{path}.{key}"))),
    }
}

fn index(v: &Value, key: &str, path: &str) -> Result<u32, VrmError> {
    let n = v
        .get(key)
        .ok_or_else(|| VrmError::Missing(format!("{path}.{key}")))?
        .as_u64()
        .ok_or_else(|| VrmError::BadValue(format!("{path}.{key}")))?;
    u32::try_from(n).map_err(|_| VrmError::BadValue(format!("{path}.{key}")))
}

fn read_meta(m: &Value) -> Result<VrmMeta, VrmError> {
    let p = "meta";
    Ok(VrmMeta {
        name: path_str(m, "name", p)?.ok_or_else(|| VrmError::Missing("meta.name".into()))?,
        version: path_str(m, "version", p)?,
        authors: path_strings(m, "authors", p)?,
        copyright_information: path_str(m, "copyrightInformation", p)?,
        contact_information: path_str(m, "contactInformation", p)?,
        references: path_strings(m, "references", p)?,
        third_party_licenses: path_str(m, "thirdPartyLicenses", p)?,
        license_url: path_str(m, "licenseUrl", p)?
            .ok_or_else(|| VrmError::Missing("meta.licenseUrl".into()))?,
        avatar_permission: path_enum(
            m,
            "avatarPermission",
            p,
            &["onlyAuthor", "onlySeparatelyLicensedPerson", "everyone"],
        )?,
        allow_excessively_violent_usage: path_bool(m, "allowExcessivelyViolentUsage", p)?,
        allow_excessively_sexual_usage: path_bool(m, "allowExcessivelySexualUsage", p)?,
        commercial_usage: path_enum(
            m,
            "commercialUsage",
            p,
            &["personalNonProfit", "personalProfit", "corporation"],
        )?,
        allow_political_or_religious_usage: path_bool(m, "allowPoliticalOrReligiousUsage", p)?,
        allow_antisocial_or_hate_usage: path_bool(m, "allowAntisocialOrHateUsage", p)?,
        credit_notation: path_enum(m, "creditNotation", p, &["required", "unnecessary"])?,
        allow_redistribution: path_bool(m, "allowRedistribution", p)?,
        modification: path_enum(
            m,
            "modification",
            p,
            &[
                "prohibited",
                "allowModification",
                "allowModificationRedistribution",
            ],
        )?,
        other_license_url: path_str(m, "otherLicenseUrl", p)?,
    })
}

fn read_expressions(
    table: Option<&Value>,
    preset: bool,
    out: &mut Vec<VrmExpression>,
) -> Result<(), VrmError> {
    let Some(table) = table else { return Ok(()) };
    let kind = if preset { "preset" } else { "custom" };
    let map = table
        .as_object()
        .ok_or_else(|| VrmError::BadValue(format!("expressions.{kind}")))?;
    let mut names: Vec<&String> = map.keys().collect();
    names.sort();
    for name in names {
        let e = &map[name];
        let path = format!("expressions.{kind}.{name}");
        let mut binds = Vec::new();
        if let Some(arr) = e.get("morphTargetBinds") {
            let arr = arr
                .as_array()
                .ok_or_else(|| VrmError::BadValue(format!("{path}.morphTargetBinds")))?;
            for b in arr {
                let bp = format!("{path}.morphTargetBinds");
                #[allow(clippy::cast_possible_truncation)]
                let weight = b
                    .get("weight")
                    .ok_or_else(|| VrmError::Missing(format!("{bp}.weight")))?
                    .as_f64()
                    .ok_or_else(|| VrmError::BadValue(format!("{bp}.weight")))?
                    as f32;
                binds.push(MorphBind {
                    node: index(b, "node", &bp)?,
                    index: index(b, "index", &bp)?,
                    weight,
                });
            }
        }
        out.push(VrmExpression {
            name: name.clone(),
            preset,
            binds,
            is_binary: path_bool(e, "isBinary", &path)?,
        });
    }
    Ok(())
}

/// Reads the `VRMC_vrm` extension of a glTF JSON chunk
///
/// # Errors
///
/// [`VrmError::Json`], [`VrmError::Vrm0`], [`VrmError::NotVrm`], [`VrmError::Missing`] or
/// [`VrmError::BadValue`]
pub fn read_vrm_json(json: &str) -> Result<VrmDocument, VrmError> {
    let v: Value = serde_json::from_str(json).map_err(|e| VrmError::Json(e.to_string()))?;
    let ext = v.get("extensions");
    let Some(vrm) = ext.and_then(|e| e.get("VRMC_vrm")) else {
        return Err(if ext.and_then(|e| e.get("VRM")).is_some() {
            VrmError::Vrm0
        } else {
            VrmError::NotVrm
        });
    };
    let spec_version = path_str(vrm, "specVersion", "VRMC_vrm")?
        .ok_or_else(|| VrmError::Missing("specVersion".into()))?;
    let meta = read_meta(
        vrm.get("meta")
            .ok_or_else(|| VrmError::Missing("meta".into()))?,
    )?;
    let mut bones = Vec::new();
    if let Some(hb) = vrm.get("humanoid").and_then(|h| h.get("humanBones")) {
        let map = hb
            .as_object()
            .ok_or_else(|| VrmError::BadValue("humanoid.humanBones".into()))?;
        for (name, b) in map {
            bones.push((
                name.clone(),
                index(b, "node", &format!("humanoid.humanBones.{name}"))?,
            ));
        }
        bones.sort();
    }
    let mut expressions = Vec::new();
    let ex = vrm.get("expressions");
    read_expressions(ex.and_then(|e| e.get("preset")), true, &mut expressions)?;
    read_expressions(ex.and_then(|e| e.get("custom")), false, &mut expressions)?;
    let look_at = vrm.get("lookAt").map(read_look_at).transpose()?;
    Ok(VrmDocument {
        spec_version,
        meta,
        bones,
        expressions,
        look_at,
    })
}

/// One primitive of a mesh: the vertex data a renderer needs
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeshPrimitive {
    /// `POSITION`
    pub positions: Vec<[f32; 3]>,
    /// `NORMAL`
    pub normals: Option<Vec<[f32; 3]>>,
    /// `TEXCOORD_0`
    pub uvs: Option<Vec<[f32; 2]>>,
    /// `JOINTS_0` (indices into the skin's joints)
    pub joints: Option<Vec<[u16; 4]>>,
    /// `WEIGHTS_0` (normalized integers converted to floats)
    pub weights: Option<Vec<[f32; 4]>>,
    /// `indices`
    pub indices: Option<Vec<u32>>,
    /// Morph target position deltas, one list per target
    pub targets: Vec<Vec<[f32; 3]>>,
    /// Morph target normal deltas, one entry per target (`None` when the target has none)
    pub target_normals: Vec<Option<Vec<[f32; 3]>>>,
}

/// A skin: its joint nodes and their inverse bind matrices (column-major)
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Skin {
    /// Node indices of the joints
    pub joints: Vec<u32>,
    /// One column-major 4x4 matrix per joint (identity when the file gives none)
    pub inverse_bind_matrices: Vec<[f32; 16]>,
}

/// A node of the scene
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// `name`
    pub name: Option<String>,
    /// `mesh`
    pub mesh: Option<u32>,
    /// `skin`
    pub skin: Option<u32>,
    /// `children`
    pub children: Vec<u32>,
    /// `translation` (default 0)
    pub translation: [f32; 3],
    /// `rotation` as a quaternion x, y, z, w (default identity)
    pub rotation: [f32; 4],
    /// `scale` (default 1)
    pub scale: [f32; 3],
}

/// A VRM file read whole
#[derive(Debug, Clone, PartialEq)]
pub struct VrmModel {
    /// The `VRMC_vrm` extension
    pub document: VrmDocument,
    /// The meshes, each a list of primitives
    pub meshes: Vec<Vec<MeshPrimitive>>,
    /// The skins
    pub skins: Vec<Skin>,
    /// The nodes
    pub nodes: Vec<Node>,
}

struct Accessors<'a> {
    json: &'a Value,
    bin: &'a [u8],
}

impl Accessors<'_> {
    /// The bytes of each element of accessor `i`: `(element bytes, count)`; checked against the BIN chunk
    fn elements(
        &self,
        i: u32,
        want_type: &str,
        comps: &[u64],
    ) -> Result<(Vec<&[u8]>, u64), VrmError> {
        let path = format!("accessors[{i}]");
        let acc = self
            .json
            .get("accessors")
            .and_then(|a| a.get(i as usize))
            .ok_or_else(|| VrmError::OutOfRange(path.clone()))?;
        if acc.get("sparse").is_some() {
            return Err(VrmError::BadValue(format!("{path}.sparse")));
        }
        let ty = acc
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| VrmError::Missing(format!("{path}.type")))?;
        if ty != want_type {
            return Err(VrmError::BadValue(format!("{path}.type")));
        }
        let ctype = acc
            .get("componentType")
            .and_then(Value::as_u64)
            .ok_or_else(|| VrmError::Missing(format!("{path}.componentType")))?;
        if !comps.contains(&ctype) {
            return Err(VrmError::BadValue(format!("{path}.componentType")));
        }
        let count = acc
            .get("count")
            .and_then(Value::as_u64)
            .ok_or_else(|| VrmError::Missing(format!("{path}.count")))?;
        let ncomp: u64 = match ty {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" | "MAT2" => 4,
            "MAT3" => 9,
            "MAT4" => 16,
            _ => return Err(VrmError::BadValue(format!("{path}.type"))),
        };
        let csize: u64 = match ctype {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            _ => 4,
        };
        let elem = ncomp * csize;
        let vi = index(acc, "bufferView", &path)?;
        let vpath = format!("bufferViews[{vi}]");
        let view = self
            .json
            .get("bufferViews")
            .and_then(|v| v.get(vi as usize))
            .ok_or_else(|| VrmError::OutOfRange(vpath.clone()))?;
        if view.get("buffer").and_then(Value::as_u64) != Some(0) {
            return Err(VrmError::BadValue(format!("{vpath}.buffer")));
        }
        let vstart = view.get("byteOffset").and_then(Value::as_u64).unwrap_or(0);
        let vlen = view
            .get("byteLength")
            .and_then(Value::as_u64)
            .ok_or_else(|| VrmError::Missing(format!("{vpath}.byteLength")))?;
        let stride = view
            .get("byteStride")
            .and_then(Value::as_u64)
            .unwrap_or(elem);
        if stride < elem {
            return Err(VrmError::BadValue(format!("{vpath}.byteStride")));
        }
        let vend = vstart
            .checked_add(vlen)
            .ok_or_else(|| VrmError::OutOfRange(vpath.clone()))?;
        if vend > self.bin.len() as u64 {
            return Err(VrmError::OutOfRange(vpath));
        }
        let astart = vstart + acc.get("byteOffset").and_then(Value::as_u64).unwrap_or(0);
        // the last element ends inside the view; checked before anything is allocated
        let need = if count == 0 {
            0
        } else {
            (count - 1)
                .checked_mul(stride)
                .and_then(|x| x.checked_add(elem))
                .ok_or_else(|| VrmError::OutOfRange(path.clone()))?
        };
        if astart.checked_add(need).is_none_or(|e| e > vend) {
            return Err(VrmError::OutOfRange(path));
        }
        #[allow(clippy::cast_possible_truncation)]
        let out = (0..count)
            .map(|k| {
                let s = (astart + k * stride) as usize;
                &self.bin[s..s + elem as usize]
            })
            .collect();
        Ok((out, ctype))
    }

    fn floats<const N: usize>(&self, i: u32, ty: &str) -> Result<Vec<[f32; N]>, VrmError> {
        let (els, _) = self.elements(i, ty, &[5126])?;
        Ok(els
            .iter()
            .map(|e| {
                std::array::from_fn(|c| {
                    f32::from_le_bytes([e[4 * c], e[4 * c + 1], e[4 * c + 2], e[4 * c + 3]])
                })
            })
            .collect())
    }

    fn weights(&self, i: u32) -> Result<Vec<[f32; 4]>, VrmError> {
        let (els, ct) = self.elements(i, "VEC4", &[5126, 5121, 5123])?;
        Ok(els
            .iter()
            .map(|e| {
                std::array::from_fn(|c| match ct {
                    5126 => {
                        f32::from_le_bytes([e[4 * c], e[4 * c + 1], e[4 * c + 2], e[4 * c + 3]])
                    }
                    5121 => f32::from(e[c]) / 255.0,
                    _ => f32::from(u16::from_le_bytes([e[2 * c], e[2 * c + 1]])) / 65535.0,
                })
            })
            .collect())
    }

    fn joints(&self, i: u32) -> Result<Vec<[u16; 4]>, VrmError> {
        let (els, ct) = self.elements(i, "VEC4", &[5121, 5123])?;
        Ok(els
            .iter()
            .map(|e| {
                std::array::from_fn(|c| {
                    if ct == 5121 {
                        u16::from(e[c])
                    } else {
                        u16::from_le_bytes([e[2 * c], e[2 * c + 1]])
                    }
                })
            })
            .collect())
    }

    fn indices(&self, i: u32) -> Result<Vec<u32>, VrmError> {
        let (els, ct) = self.elements(i, "SCALAR", &[5121, 5123, 5125])?;
        Ok(els
            .iter()
            .map(|e| match ct {
                5121 => u32::from(e[0]),
                5123 => u32::from(u16::from_le_bytes([e[0], e[1]])),
                _ => u32::from_le_bytes([e[0], e[1], e[2], e[3]]),
            })
            .collect())
    }
}

fn opt_attr(attrs: &Value, key: &str, path: &str) -> Result<Option<u32>, VrmError> {
    if attrs.get(key).is_none() {
        return Ok(None);
    }
    index(attrs, key, path).map(Some)
}

fn floats_n<const N: usize>(
    v: &Value,
    key: &str,
    default: [f32; N],
    path: &str,
) -> Result<[f32; N], VrmError> {
    let Some(a) = v.get(key) else {
        return Ok(default);
    };
    let a = a
        .as_array()
        .filter(|a| a.len() == N)
        .ok_or_else(|| VrmError::BadValue(format!("{path}.{key}")))?;
    let mut out = default;
    for (o, x) in out.iter_mut().zip(a) {
        #[allow(clippy::cast_possible_truncation)]
        let f = x
            .as_f64()
            .ok_or_else(|| VrmError::BadValue(format!("{path}.{key}")))? as f32;
        *o = f;
    }
    Ok(out)
}

fn read_primitive(acc: &Accessors<'_>, p: &Value, path: &str) -> Result<MeshPrimitive, VrmError> {
    let attrs = p
        .get("attributes")
        .ok_or_else(|| VrmError::Missing(format!("{path}.attributes")))?;
    let apath = format!("{path}.attributes");
    let positions = acc.floats::<3>(index(attrs, "POSITION", &apath)?, "VEC3")?;
    let n = positions.len();
    let same = |len: usize, what: &str| {
        if len == n {
            Ok(())
        } else {
            Err(VrmError::BadValue(format!("{apath}.{what}")))
        }
    };
    let normals = opt_attr(attrs, "NORMAL", &apath)?
        .map(|i| acc.floats::<3>(i, "VEC3"))
        .transpose()?;
    let uvs = opt_attr(attrs, "TEXCOORD_0", &apath)?
        .map(|i| acc.floats::<2>(i, "VEC2"))
        .transpose()?;
    let joints = opt_attr(attrs, "JOINTS_0", &apath)?
        .map(|i| acc.joints(i))
        .transpose()?;
    let weights = opt_attr(attrs, "WEIGHTS_0", &apath)?
        .map(|i| acc.weights(i))
        .transpose()?;
    for (len, what) in [
        (normals.as_ref().map(Vec::len), "NORMAL"),
        (uvs.as_ref().map(Vec::len), "TEXCOORD_0"),
        (joints.as_ref().map(Vec::len), "JOINTS_0"),
        (weights.as_ref().map(Vec::len), "WEIGHTS_0"),
    ] {
        if let Some(len) = len {
            same(len, what)?;
        }
    }
    let indices = opt_attr(p, "indices", path)?
        .map(|i| acc.indices(i))
        .transpose()?;
    if let Some(ix) = &indices {
        if ix.iter().any(|&i| i as usize >= n) {
            return Err(VrmError::OutOfRange(format!("{path}.indices")));
        }
    }
    let mut targets = Vec::new();
    let mut target_normals = Vec::new();
    if let Some(ts) = p.get("targets") {
        let ts = ts
            .as_array()
            .ok_or_else(|| VrmError::BadValue(format!("{path}.targets")))?;
        for (k, t) in ts.iter().enumerate() {
            let tp = format!("{path}.targets[{k}]");
            let d = acc.floats::<3>(index(t, "POSITION", &tp)?, "VEC3")?;
            same(d.len(), &format!("targets[{k}].POSITION"))?;
            let tn = opt_attr(t, "NORMAL", &tp)?
                .map(|i| acc.floats::<3>(i, "VEC3"))
                .transpose()?;
            if let Some(tn) = &tn {
                same(tn.len(), &format!("targets[{k}].NORMAL"))?;
            }
            targets.extend([d]);
            target_normals.extend([tn]);
        }
    }
    Ok(MeshPrimitive {
        positions,
        normals,
        uvs,
        joints,
        weights,
        indices,
        targets,
        target_normals,
    })
}

/// Reads a VRM 1.0 file whole
///
/// # Errors
///
/// Any [`VrmError`]: the container, the extension or the mesh data does not read
pub fn read_vrm(data: &[u8]) -> Result<VrmModel, VrmError> {
    let glb = split_glb(data)?;
    let document = read_vrm_json(glb.json)?;
    let json: Value = serde_json::from_str(glb.json).map_err(|e| VrmError::Json(e.to_string()))?;
    let acc = Accessors {
        json: &json,
        bin: glb.bin.unwrap_or(&[]),
    };
    let list = |key: &str| -> Result<Vec<Value>, VrmError> {
        match json.get(key) {
            None => Ok(Vec::new()),
            Some(Value::Array(a)) => Ok(a.clone()),
            Some(_) => Err(VrmError::BadValue(key.to_owned())),
        }
    };
    let mut meshes = Vec::new();
    for (m, mesh) in list("meshes")?.iter().enumerate() {
        let prims = mesh
            .get("primitives")
            .and_then(Value::as_array)
            .ok_or_else(|| VrmError::Missing(format!("meshes[{m}].primitives")))?;
        let mut out = Vec::new();
        for (k, p) in prims.iter().enumerate() {
            out.push(read_primitive(
                &acc,
                p,
                &format!("meshes[{m}].primitives[{k}]"),
            )?);
        }
        meshes.push(out);
    }
    let nodes_json = list("nodes")?;
    let (skins, nodes) = read_skins_and_nodes(&acc, &nodes_json, &list("skins")?, meshes.len())?;
    let node_count = nodes.len();
    let check_node = |i: u32, p: &str| {
        if (i as usize) < node_count {
            Ok(i)
        } else {
            Err(VrmError::OutOfRange(p.to_owned()))
        }
    };
    for (b, n) in &document.bones {
        check_node(*n, &format!("humanoid.humanBones.{b}"))?;
    }
    for e in &document.expressions {
        for bind in &e.binds {
            let node = nodes
                .get(bind.node as usize)
                .ok_or_else(|| VrmError::OutOfRange(format!("expressions.{}.node", e.name)))?;
            let targets = node
                .mesh
                .and_then(|m| meshes.get(m as usize))
                .map_or(0, |prims| {
                    prims.iter().map(|p| p.targets.len()).max().unwrap_or(0)
                });
            if bind.index as usize >= targets {
                return Err(VrmError::OutOfRange(format!(
                    "expressions.{}.index",
                    e.name
                )));
            }
        }
    }
    Ok(VrmModel {
        document,
        meshes,
        skins,
        nodes,
    })
}

fn read_skins_and_nodes(
    acc: &Accessors<'_>,
    nodes_json: &[Value],
    skins_json: &[Value],
    mesh_count: usize,
) -> Result<(Vec<Skin>, Vec<Node>), VrmError> {
    let node_count = nodes_json.len();
    let check_node = |i: u32, p: &str| {
        if (i as usize) < node_count {
            Ok(i)
        } else {
            Err(VrmError::OutOfRange(p.to_owned()))
        }
    };
    let mut skins = Vec::new();
    for (s, skin) in skins_json.iter().enumerate() {
        let path = format!("skins[{s}]");
        let joints: Vec<u32> = skin
            .get("joints")
            .and_then(Value::as_array)
            .ok_or_else(|| VrmError::Missing(format!("{path}.joints")))?
            .iter()
            .map(|j| {
                j.as_u64()
                    .and_then(|j| u32::try_from(j).ok())
                    .ok_or_else(|| VrmError::BadValue(format!("{path}.joints")))
                    .and_then(|j| check_node(j, &format!("{path}.joints")))
            })
            .collect::<Result<_, _>>()?;
        let ibm = match opt_attr(skin, "inverseBindMatrices", &path)? {
            Some(i) => acc.floats::<16>(i, "MAT4")?,
            None => vec![std::array::from_fn(|k| if k % 5 == 0 { 1.0 } else { 0.0 }); joints.len()],
        };
        if ibm.len() != joints.len() {
            return Err(VrmError::BadValue(format!("{path}.inverseBindMatrices")));
        }
        skins.push(Skin {
            joints,
            inverse_bind_matrices: ibm,
        });
    }
    let mut nodes = Vec::new();
    for (i, n) in nodes_json.iter().enumerate() {
        let path = format!("nodes[{i}]");
        let mesh = opt_attr(n, "mesh", &path)?;
        if mesh.is_some_and(|m| m as usize >= mesh_count) {
            return Err(VrmError::OutOfRange(format!("{path}.mesh")));
        }
        let skin = opt_attr(n, "skin", &path)?;
        if skin.is_some_and(|s| s as usize >= skins.len()) {
            return Err(VrmError::OutOfRange(format!("{path}.skin")));
        }
        let children = match n.get("children") {
            None => Vec::new(),
            Some(c) => c
                .as_array()
                .ok_or_else(|| VrmError::BadValue(format!("{path}.children")))?
                .iter()
                .map(|c| {
                    c.as_u64()
                        .and_then(|c| u32::try_from(c).ok())
                        .ok_or_else(|| VrmError::BadValue(format!("{path}.children")))
                        .and_then(|c| check_node(c, &format!("{path}.children")))
                })
                .collect::<Result<_, _>>()?,
        };
        nodes.push(Node {
            name: path_str(n, "name", &path)?,
            mesh,
            skin,
            children,
            translation: floats_n(n, "translation", [0.0; 3], &path)?,
            rotation: floats_n(n, "rotation", [0.0, 0.0, 0.0, 1.0], &path)?,
            scale: floats_n(n, "scale", [1.0; 3], &path)?,
        });
    }
    Ok((skins, nodes))
}
