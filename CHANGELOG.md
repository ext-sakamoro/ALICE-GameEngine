# Changelog

## [Unreleased]

### 破壊的変更

- `vrm::VrmDocument::meta` は `vrm::Licence` (`Vrm1(VrmMeta)` / `Vrm0(vrm0::Vrm0Meta)`) になった 0.x の利用条件の項目は 1.0 の項目に移せないので、どちらも他方から作らず書かれたまま持つ 名前と最初の作者は `Licence::title` / `Licence::author` `VrmError::Vrm0` は無くなった (0.x は読む)
- `vrm_render::ToonParams` の `threshold` を MToon の `shading_shift` と `shading_toony` に替えた (以前の `threshold: x` は `shading_shift: -x, shading_toony: 1.0` と同じ描画)
- `humanoid::HumanoidBone` を VRM 1.0 の 55 骨 (指 30 本を含む) に、`ExpressionChannel` を VRM 1.0 の 18 preset (`Relaxed` / `Blink` / `LookUp` / `LookDown` / `LookLeft` / `LookRight` を追加) にした 網羅的な `match` は新しい variant の arm が要る 骨と表情は仕様の名前と相互に変換できる (`vrm_name` / `from_vrm_name`、`ExpressionChannel::presets`)、仕様に無い表情の名前は `Custom`
- `vrm_render::ToonPart` に `surface: Option<Surface>` が増えた struct literal は `surface: None` を足す (以前と同じ描画) `ToonTexture` は `key` を持つ
- `ExpressionSet::iter` は channel の順 (preset は enum の順、custom は名前の順) に並ぶ (以前は HashMap の順で決まっていなかった)

### Added
- `vrm_render::ToonRenderer::with_device(device, queue, format, width, height)` は呼び手の device (browser の canvas・window) で作る (`new_headless` はこれを Rgba8Unorm で呼ぶ) `draw_parts_into(parts, clear, view)` はその format の view に描いて submit する (readback 無し) `draw_parts` は Rgba8Unorm の renderer だけが読み戻し、他は `RenderError::Readback` `resize(width, height)` で以後の画像の寸法を変える (1..=8192) `ToonTexture::key` は呼び手がその bytes に付ける名前で、同じ key の texture は 1 度だけ upload して以後の draw で使い回す (`forget_textures` で捨てる、`None` は毎回 upload) 試験 `tests/vrm_render_target.rs` (4 件: view に描いた bytes が readback と一致、Bgra8Unorm では赤と青が入れ替わり readback は拒否、resize 後の寸法と画素、範囲外の寸法の拒否、key の upload が 1 度で forget まで残ること) 変異 5 件が red
- texture 付きの toon (`vrm_render::Surface` と `ToonPart::surface`): 頂点ごとの UV、基本色に掛ける texture と陰の色に掛ける texture (`ToonTexture`、RGBA8 で上の行から、bytes はそのまま)、UV 変換 (`UvTransform`、`KHR_texture_transform` の `offset + rotation(scale * uv)`)、alpha の描き方 (`AlphaMode::Opaque` / `Mask(cutoff)` / `Blend`) sampler は linear で repeat mask は cutoff 未満の fragment を描かず (輪郭線も)、blend は不透明な部品の後に部品の順で深度を書かずに重ねる `check_surface` は UV の数と有限性、texture の寸法 (1..=8192) と bytes の長さ、変換の有限性、cutoff (0..=1) を検査し、draw は部品の番号つきで `RenderError::BadSurface` を返す device の上限を超える texture も同じく拒否する headless の device は adapter の 2D texture の上限を要求する (以前は downlevel の 2048 で、4096 の texture を作ると panic した) 試験 `tests/vrm_surface.rs` (13 件: 2 x 2 の texture の texel の中心を全頂点で読み、texel ごとの色・色の掛け算・offset / scale / 回転と repeat・陰の texture・mask の cutoff と奥の部品の透け・blend の混色と順序と深度・linear の filter (拡大と縮小)・両方向の repeat・4096 の texture・拒否を手で計算した値と比べる) mask の部品では輪郭線も cutoff 未満で描かない試験を `tests/vrm_outline.rs` に 1 件 変異 12 件が red CI の gpu-oracle に vrm_surface=13 と vrm_render_target=4、vrm_outline は 8
- feature `lol-measure`: `vrm_measure::rig` は VRM の model を `alice-lol-humanoid` の形式に依らない `Rig` にする (humanoid が写す node は humanoid の骨の名前と rest pose の world 位置、他の node は名前も bind も無し、skin 付き mesh は rest pose で skin した頂点とそれぞれ最も重い joint) これで VRM の体を比率の Law の単位で測れる (`alice_lol_humanoid::rig::body_marks` → `measure::measure`) 0.x の file は読み手が +Z に向けたもの 試験 `tests/vrm_measure.rs` (4 件: 手で置いた fixture の骨の名前・親・world 位置、頂点と最も重い joint、重みの無い頂点、目の無い rig は測らない) 変異 8 件が red CI に `lol-measure` job (min_passed vrm_measure=4) と preflight
- VRM 0.x (`extensions.VRM`) を同じ `VrmModel` に読む (`vrm::read_vrm` / `read_vrm_json`、`vrm0`): 人型の骨は 1.0 の名前に (親指は `thumbProximal` → `thumbMetacarpal`、`thumbIntermediate` → `thumbProximal`)、表情の preset は 1.0 の名前に (`a` → `aa`、`joy` → `happy`、`blink_l` → `blinkLeft` 等、大小文字は問わない)、`unknown` は group の名前の custom、bind の mesh はそれを持つ最初の node に、重みは 0..100 から 0..1 に 視線は `firstPerson` (`xRange` = 入力の最大、`yRange` = 出力の倍率) 材質は `materialProperties[i]` の `VRM/MToon` を 1.0 の MToon に、参照実装の移行と同じ式で換算 (色は sRGB から linear、`_ShadeShift` / `_ShadeToony` の陰の範囲から 1.0 の shift と toony、輪郭線の幅は world が cm から m、screen が画面の高さの半分に対する % から高さに対する比、lighting mix は混色の mode の時だけ) 0.x の model は −Z を向くので、読む時に Y 軸まわりに 180° 回して +Z を向ける (位置・法線・morph の差分・node の移動と回転・inverse bind matrix) 視線の offset は書き出した側の左手系で既に +Z を向いているので x の符号だけ変える 試験 `tests/vrm0_reader.rs` (8 件: 手で置いた fixture を 0.x に書き換え、1.0 として読んだものを手で回した値と、skin した頂点が回した値になることを比べる、換算の値は手で計算、不正な部品の拒否と壊れた bytes で panic しないこと) 変異 40 件が red 例 `vrm_pose_demo` は 0.x の利用条件も表示する
- 輪郭線 (MToon 1.0 の inverted hull): `vrm_render::Outline` (`OutlineMode::World` = model の単位の幅、`OutlineMode::Screen` = 画像の高さに対する幅、色、lighting mix) と `ToonPart::outline` 面を法線に沿って幅だけ押し出し、表の面を culling して描き、色は `outline_color * mix(1, 面の光の色, lighting_mix)` screen の幅は clip の w を掛けて深さに依らず、横は aspect で割って縦と同じ画素数 `Outline::from_material` は VRM の材質から作る (mode が none か幅 0 なら無し) 読み手は `vrm::MToon` に `outline_width_mode` (`OutlineWidthMode`、既定 none)、`outline_width` (既定 0、負は拒否)、`outline_color` (既定 0)、`outline_lighting_mix` (既定 1、0..=1 の外は拒否) を読む draw で部品の番号つきに検査し、拒否は `RenderError::BadOutline`
- 顔の描き込み `vrm_render::FacePaint` (`PaintShape::Ellipse` / `PaintShape::Stroke`、最大 `MAX_PAINT_SHAPES` = 32) と `ToonPart::paint`: toon shader が fragment ごとに face plane (`to_face` の行列で world から移した xy、z が `min_depth` 以上の所だけ) で形の符号付き距離を評価し、内側を後の形ほど上に、光に依らない平らな色で塗る 楕円は (|(p - c) / r| - 1) min(r)、stroke は 2 次曲線を 9 点で折れ線にした距離から半幅を引く 目・眉・睫毛・口を mesh でなく面の上の法則として持ち、表情は形の数値の変化になる `check_paint` (形の数、有限、半径と半幅が正) を draw の前に部品の番号つきで当て、拒否は `RenderError::BadPaint` 試験 `tests/vrm_face_paint.rs` (CPU 1 件 + GPU 4 件、画素は手で計算) を `gpu-oracle` と preflight に追加 (vrm_face_paint 5 件)
- `vrm_render::ToonPart` と `ToonRenderer::draw_parts`: 複数の primitive を、それぞれの材質・光・視点で 1 枚の画像に 1 つの depth buffer で描く (近い fragment が遠い fragment を部品の順に依らず隠す) 部品が無い時と、`check_mesh` が拒否する部品 (番号を添える) は `RenderError::BadMesh` `draw` は 1 部品の `draw_parts` 試験 `tests/vrm_render_parts.rs` (GPU 3 件、画素は view-projection から手で計算: 近い部品の青が両方の順で残る / 遠い部品だけの所は赤 / 外は clear、1 部品は `draw` と同じ bytes、空と不正な部品の拒否) を `gpu-oracle` と preflight の GPU step に追加 (vrm_render_parts 3 件) 例 `vrm_toon_offscreen` は最初の mesh の primitive を全て (材質ごとに) 描く

- VRM の材質: 読み手は `VrmModel::materials` (`Material`: 名前、`pbrMetallicRoughness.baseColorFactor` (既定 1)、`VRMC_materials_mtoon` の `MToon` (`shadeColorFactor` 既定 0、`shadingShiftFactor` 既定 0、`shadingToonyFactor` 既定 0.9、0..=1 の外は拒否))、`MeshPrimitive::material` (存在しない材質の index は拒否) を読む toon 描画は MToon 1.0 の陰影 (`s = dot(n, l) + shift`、`t = clamp((s + 1 - toony) / (2 - 2 toony), 0, 1)`、toony 1 で 0 の段差、色 = `mix(shade, base, t)`) にし、`ToonParams::from_material` で材質から parameter を作る 試験 `tests/vrm_mtoon.rs` (CPU 4 件 + GPU 1 件、値は fixture の材質から手で計算、GPU の試験は CI の `gpu-oracle` で lavapipe に走らせ件数を確かめる) fixture に MToon の材質を足した
- `scripts/min_passed.py --no-ignored`: 名前を挙げた binary に ignored の試験が 1 件でもあれば失敗する `gpu-oracle` と preflight の GPU step で使い、`#[ignore]` の GPU 試験が `--include-ignored` で実際に走ったことを件数と合わせて確かめる (vrm_mtoon は 5 件) 試験 `scripts/test_min_passed.py` (9 件) を同じ step で先に走らせる
- VRM の視線: 読み手は `VRMC_vrm.lookAt` を `VrmDocument::look_at` (`LookAt`: 型 `LookAtKind::Bone` / `Expression`、`offsetFromHeadBone`、4 つの `RangeMap` (仕様の既定は入力の上限 90 度・出力 10)) に読む `VrmPose::set_look(yaw_left_deg, pitch_up_deg)`: 表情の型は lookLeft / lookRight を水平の outer の map、lookUp / lookDown を垂直の map で重みに (0..=1)、骨の型は目の骨を +Y (yaw、外向きは outer・内向きは inner の map) と -X (pitch) に map した度数だけ休止の回転の後に回す 重みが 0 でない視線の表情が model に無ければ `PoseError::UnknownExpression`、lookAt が無ければ `PoseError::NoLookAt` range map は仕様に既定値が無く、省略時は参照実装と同じく両方の型で 90 → 10 (表情の重みは 0..=1 に clamp するので 9 度で飽和)、入力の上限が 0 の時は入力が 0 なら 0、それ以外は出力の scale (仕様の推奨) 試験 `tests/vrm_look_at.rs` (8 件、fixture の対応表から手で計算) fixture に lookAt と lookLeft / lookUp を足した
- `VrmPose::posed_normals`: morph target の法線の差分を掛け、混ぜた skinning 行列 (skin が無ければ node の world 行列) の 3x3 の逆転置で回し、長さを 1 に戻した法線 法線の無い primitive は `PoseError::NoNormals` `MeshPrimitive::target_normals` (morph target の NORMAL、無い target は `None`) 例 `vrm_toon_offscreen` は pose した法線で描く fixture の target 0 に法線の差分を足した
- `call_fx` module: RGBA8 の画像に通話の劣化 (block 化・色ずれ・色の bit 数・走査線・noise) を整数の算術だけで掛ける `apply` / `CallFx` (同じ画像と parameter なら全 target で同じ bytes)、映像のコマ止めの `FrameHold` 試験 `tests/call_fx.rs` (5 件、手で計算した小さな場合と、独立実装 `scripts/call_fx_reference.py` (標準 library の Python) が書いた 9 件の結果の SHA-256) CI の test job と preflight で `scripts/call_fx_reference.py --check` を走らせる 例 `vrm_toon_offscreen` に劣化の段 (0〜3) の引数を足した 開発用の依存に `sha2` を足した
- `vrm_render` module: pose した VRM の primitive を 2 階調の toon shader で画面外の画像に描く `ToonRenderer` (`gpu` feature、RGBA8 を上の行から返す) と、shader の WGSL `toon_wgsl` (naga で検査)、upload の前に mesh を検査する `check_mesh` skinning と morph は CPU の `VrmPose` の結果を使う 試験 `tests/vrm_render.rs` (shader の検査と mesh の拒否は常に、GPU の 2 件は手で計算した被覆と明暗を画素で確かめる) CI に software の adapter (Mesa の lavapipe) で `#[ignore]` の GPU 試験を走らせる job `gpu-oracle` を追加 (hd2d_postfx の GPU 試験も含む、走った件数を `scripts/min_passed.py` で確かめ、adapter が無ければ失敗)、preflight にも同じ step を追加 例 `examples/vrm_toon_offscreen.rs` (PPM を書き出す)
- `Humanoid::bind_from_vrm_document`: VRM の文書の骨を全部結び、仕様に無い骨の名前を返す (`bind_from_vrm` は黙って落としていた) `VrmPose::apply_expression_set`: `ExpressionSet` の channel を同じ VRM 名の表情に掛け、model に無い表情の名前を返す 試験 `tests/vrm_humanoid.rs` (5 件、仕様から写した骨 55 と preset 18 の名前の表で照合)
- `vrm_pose` module: VRM の model を CPU で pose する `VrmPose` (表情の重み → morph target の重み (bind の重みとの積の和を 0..=1 に clamp)、morph target、node の TRS と親子、skinning の順は glTF 2.0 のとおり morph → skin) skinning は既存の `skeleton::apply_skin_cpu` を使う node の循環・2 つの親は `PoseError::NodeCycle`、範囲外・非有限の入力は error 試験 `tests/vrm_pose.rs` (6 件、値は fixture の数から手で計算) 例 `examples/vrm_pose_demo.rs` (license の欄・表情ごと・骨を動かした時の pose を表示)
- `vrm` module: VRM 1.0 の読み手 `read_vrm` (GLB の JSON / BIN chunk、`VRMC_vrm` の meta を license の全項目で (`VrmMeta`、仕様の既定値と値の集合で検査、範囲外は `VrmError::BadValue`、`licenseUrl` 無しは `Missing`)、humanoid の骨、全ての表情の morph target の bind (preset と custom)、mesh の POSITION / NORMAL / TEXCOORD_0 / JOINTS_0 / WEIGHTS_0 (正規化整数を含む) / indices / morph target の差分、skin と逆 bind 行列、node) `split_glb` / `read_vrm_json` も公開 長さと offset は使う前に bytes と照らし、file が言う数で確保しない 壊れた file は `VrmError`、VRM 0.x は `VrmError::Vrm0` 試験 `tests/vrm_reader.rs` (9 件) は `scripts/vrm_fixture.py` (標準 library の Python、仕様から bytes を手で並べる) が書いた `tests/data/vrm/tiny.vrm` と期待値 `expected.txt` で照合し、全ての前置部分と全 byte の反転で panic しないこと、範囲外の参照の拒否を確かめる CI の test job と preflight で `scripts/vrm_fixture.py --check` を走らせる

### Changed — dependencies

- 依存: `alice-sdf` を registry の 1.9.2 から git 依存 (rev `3c7367f`、7.0.0) に変更
- 依存: `alice-physics` の git 依存を rev `391971e6` (4.0.0) に変更
- CI: `physics` feature の clippy と test を ubuntu で実行する job を追加 (test は binary ごとの最小件数 lib 1044 / physics_hub_analytic 23 / physics_hub_degenerate 10 を `scripts/min_passed.py` で検査)、preflight にも同じ step を追加

### Fixed
- VRM / glTF の node の `matrix` (列優先 4x4) を読む 以前は translation / rotation / scale だけを読み、`matrix` で書かれた node は黙って単位行列になっていた `matrix` を T · R · S に分解し (列の長さが scale、正規化した列が回転、行列式が負なら x の scale を負に、四元数は w を負にしない側)、仕様が許さないものは `BadValue` (`nodes[i].matrix`) で断る: TRS と同時に書かれたもの、有限でない値、最後の行が 0 0 0 1 でないもの、長さ 0 の列、直交しない列 (shear) 0.x の file では他の node と同じく 180° 回す 試験 `tests/vrm_node_matrix.rs` (6 件: 手で組んだ行列が元の TRS に戻り、pose した world 行列が与えた行列に一致する、四元数の 4 つの分岐を w が 0 でない回転と全成分が 0 でない回転で全て通る、鏡像、0.x、断る行列) 四元数は長さ 1 に直す 長さ 0 の列 (scale 0) は glTF では許されるが、その軸の回転が決まらないので推測せず断る 変異 26 件が red

- `call_fx` の doc に noise の分布 (65536 値を 2a + 1 に畳む剰余の偏りを除いて一様、`a` は `u8` なので 2a + 1 は最大 511) を書いた
- `vrm_pose::VrmPose::new` は範囲外の子の index を panic でなく `PoseError::NoSuchNode` で拒否する (公開の `VrmModel` を手で組んだ場合) `set_rotation` は長さの 2 乗が 1 から 1e-4 を超えて離れた quaternion を `PoseError::NotUnitRotation` で拒否する (glTF の回転は単位 quaternion)
- `window` feature を単独で有効にすると build できなかった (`ui_renderer` が `window` だけで gate され、`ui` feature の `crate::ui` を使っていた) `ui_renderer` を `window` と `ui` の両方で gate する
- CI: Cargo.toml の各 feature を単独で `cargo check --lib --no-default-features --features <f>` する job と `scripts/each_feature.py` を追加 (feature の一覧は Cargo.toml から読む、check の失敗と 0 件は失敗、`default` / `full` は理由付きで除外 (`python` / `godot` は `ffi` だけを有効にする feature なので check に含める)、Linux では `audio_output` のために ALSA の header を入れる)、preflight にも同じ step を追加
- `parse_vrm_json` / `parse_vrm_full` を `vrm::read_vrm_json` の上で読む (JSON を 2 か所で読まない) 表情の重みは従来どおり最初の bind の値 meta の必須項目が欠けた・値が仕様の外の chunk は `None` になる
- 依存: `alice-physics` の git 依存 (rev `51b34e4`) に `version = "1.5"` を書いた (その rev の版は 1.5.0) version の無い git 依存は publish する crate で版に置き換えられず、固定した commit の版とも比べられない 解決される commit は変わらない

### Changed — breaking: rigid-body physics delegated to ALICE-Physics (`physics` feature)

- **Breaking:** `physics3d` / `collision` / `joint` moved behind the new
  `physics` feature (off by default, not part of `full`). It pulls in
  `alice-physics` (`AGPL-3.0-or-later OR LicenseRef-Commercial`), so a
  distributed build with this feature is subject to those terms
- **Breaking:** `physics3d` is handle based: `PhysicsWorld::add_body(BodyDesc)`
  returns a `BodyHandle` (index + generation), and bodies are read and changed
  through `position` / `set_velocity` / `apply_impulse` / ... The old
  `RigidBody` type with pub fields, `PhysicsWorld::bodies` / `contacts` /
  `gravity` fields, `step(dt)`, `broadphase*` and `resolve_contact` are removed
- **Breaking:** `joint::solve_joints` (direct position writes) is removed;
  joints are added with `PhysicsWorld::add_joint` / `remove_joint` and solved
  by the inner world. `Joint::body_a` / `body_b` are `BodyHandle`s,
  `RagdollDef` holds handles
- **Breaking:** `collision::gjk` lost its `max_iterations` argument and
  `GjkResult::Indeterminate` (two values: `Intersecting` / `Separated`)
- **Breaking:** `easy::Game::add_physics_body` needs the `physics` feature and
  returns a `BodyHandle`
- **Behavior change:** integration is ALICE-Physics XPBD in 128-bit fixed point
  with substeps; `PhysicsWorld::update(frame_dt)` runs whole fixed steps
  (results do not depend on the frame rate)
- **Behavior change:** `linear_damping` / `angular_damping` mean the fraction
  of velocity lost per second
- **Behavior change:** `PhysicsConfig::default()` is gravity `(0, -9.81, 0)`,
  `fixed_dt` 1/60, `substeps` 4, `max_steps_per_update` 8, `broadphase`
  `DynamicTree` (1000 spheres: about 12 ms per step on an arm64 10-core
  machine)
- **Behavior change:** a body collides only with a positive `radius`; joints
  are read in world space at creation and turn with the bodies; hinge and
  cone-twist pivot at body A (the cone-twist twist limit is now enforced)
- **Behavior change:** `collision::mesh_vs_sdf` returns a unit normal (it
  used to pass `sdf_normal` through unnormalised)

### Fixed

- Two overlapping bodies passed through each other and swapped places (the
  contact normal and the resolution used opposite directions)
- `apply_impulse` and contact impulses had no effect on the next step (the
  integrator did not read the velocity)
- `easy::Game::add_physics_body` added to a temporary world and always
  returned 0; bodies now live in `EngineContext::physics`
- MCP `physics_add_body` was listed but not implemented, and `physics_step`
  only advanced the clock; both now act on `EngineContext::physics`
- `joint::build_ragdoll` joined consecutive bones with zero anchors (bones
  collapsed onto one point); the pivot is now the parent bone

### Added

- `physics3d::BodyHandle` / `JointHandle` (`to_bits` / `from_bits` for text
  protocols), `BodyDesc`, `BodyKind`, `PhysicsConfig`, `Broadphase`
- `PhysicsWorld::contacts()` (normal from `body_a` to `body_b`),
  `set_kinematic_target`, `apply_force` / `apply_torque` (act over the next
  fixed step), `is_sleeping` / `wake`, `total_kinetic_energy`, `inner()`
- `collision::contact` (EPA penetration, `ConvexContact`)
- `impl CollisionProvider for PhysicsWorld` (sphere cast / AABB overlap with
  each body's own radius)
- `EngineContext::physics`, set from `EngineConfig` (gravity, fixed step, step
  cap) and advanced by `Engine::frame`
- `physics3d::sdf_ccd` delegates to `alice_physics::sdf_ccd::sphere_trace_sdf_field`
  through `DistanceSdfQuery` (the normal comes from ALICE-Physics' central
  differences); `collision::mesh_vs_sdf` to
  `alice_physics::sdf_collider::collide_point_sdf_field`. A sphere that starts
  inside the surface hits at `time_of_impact = 0`
- Examples `fps` / `racing` / `sandbox` registered (`physics` feature);
  `physics_sandbox` and `constraint_demo` now require `physics`

### Changed — breaking: MCP handshake の実装に伴う `mcp` module の型変更

- **`initialize` を実装** — これが無いため MCP client は接続を完了できず、実装済の
  8 tool は実際には到達不能だった `protocolVersion` は client の要求を echo し、
  省略時は新 const `MCP_PROTOCOL_VERSION` (`2024-11-05`) を返す `capabilities.tools`
  と `serverInfo` (`alice-game-engine` / `CARGO_PKG_VERSION`) を宣言する
- **`ping` を実装** (spec の標準 method、空 result)
- **notification に応答しなくなった** — JSON-RPC 2.0 は id の無い要求への応答を禁じて
  おり、`notifications/initialized` に応答すると client が session を切る これに伴い
  **`McpRequest.id` が `u64` → `Option<u64>`**、**`McpHandler::handle` が
  `McpResponse` → `Option<McpResponse>`** (breaking、ただし consumer は本 crate 内の
  `easy::Game::mcp_call` / `examples/mcp_controlled` / `bin/mcp_server` のみ)
- stdio loop は `None` を受けたら何も書かずに次の行へ進む
- 検証: stdio に `initialize` → `notifications/initialized` → `ping` → `tools/list` を
  流して**応答 3 行** (= notification に応答していない) / `tools` 8 個 / echo された
  `protocolVersion` を確認 test 5 本追加 (handshake / notification / ping)

### Changed — SDF law delegated to `alice-sdf` (single source)

- `sdf::SdfPrimitive::eval` and `sdf::apply_op` no longer carry their own
  distance / smooth-blend formulas: they call `alice_sdf::primitives::sdf_*`
  and `alice_sdf::operations::sdf_smooth_*` (new dependency
  `alice-sdf = "1.9.2"`, `default-features = false`). Engine parameter
  conventions are unchanged (`height` = full height, `Cone` base at `y = 0`);
  cone distances are now exact instead of the previous approximation (same
  zero set).
- `impl From<&SdfPrimitive> / From<&SdfNode> for alice_sdf::SdfNode` —
  lossless conversion into the ALICE-SDF tree.
- `sdf::marching_cubes` / `marching_cubes_parallel` / `dual_contouring` now
  delegate to `alice_sdf::mesh::{marching_cubes, dual_contouring}`; the
  engine's own edge / triangle tables and DC solver (~700 lines) are removed.
  `marching_cubes_parallel` is an alias of `marching_cubes` (ALICE-SDF's
  implementation is Z-slab parallel already). `adaptive_marching_cubes`
  keeps its octree driver but meshes leaves through the delegated function.
- `sdf2d::Sdf2dPrimitive::eval` / `Sdf2dOp::SmoothUnion` delegate to
  `alice_sdf::primitives::{sdf_circle_2d, sdf_rect_2d, sdf_rounded_rect_2d,
  sdf_segment_2d, sdf_polygon_2d_xy}` and `operations::smooth_min` (extruded
  shapes evaluated on `z = 0` with an unbounded half height, where the
  extrusion is the identity; the triangle is the exact 3-vertex polygon
  distance). Engine conventions kept (`Segment::thickness` = full width).
  `sdf2d::alice_sdf_parity` pins the 2D law to the extruded ALICE-SDF nodes.
- Tests: `sdf::alice_sdf_parity` (every primitive / op / transform vs
  `alice_sdf::eval` on 500 points) and `simd_eval::alice_sdf_lane_parity`
  (`sdf_sphere_x8` / `sdf_box_x8` lanes vs `alice_sdf::primitives`; the SIMD
  helpers remain local copies because ALICE-SDF has no scalar-function SIMD
  API).

### New modules

- **`environment_trajectory`** — Language world model / agent simulator
  trait. `EnvironmentTrajectory: Send + Sync` with
  `predict_next(history, action) → Observation` + `schema()` +
  `is_ready()`. Opaque `Action::Custom { kind, payload }` and
  `Observation::Custom { kind, payload }` enums for forward
  compatibility, `Turn { action, observation }` for trajectory storage,
  `EnvironmentSchema { task_description_hash, action_space_kinds,
  stateful }` for the static descriptor. `MockEnvironmentTrajectory`
  with `echo` / `fixed` / `xor` behaviours co-located in the module.
  Eight unit tests + one doctest, including a `Box<dyn Trait>` contract
  test. Enables ALICE-LLM / ALICE-Cognitive / ALICE-Metaverse and
  third-party language world models to plug in as NPC simulators or
  Sim RL environments without owning the inference backend.
- **`decal`** — Deferred decal projection (Wicked-inspired). `DecalData`
  (albedo / normal / color / opacity / layer_mask / blend_mode),
  `DecalBlendMode` (AlphaBlend / Multiply / Additive with stable shader
  IDs), `DecalDraw` with pre-computed inverse world matrix and OBB→AABB
  helper for frustum culling.

### Existing modules — additions

- **`joint`** — Three new `JointKind` variants (Wicked-inspired
  `ConstraintComponent`):
  - `Slider` (prismatic, 1-axis slide with min/max offset clamp)
  - `Fixed` (weld, holds `B - A == offset`)
  - `ConeTwist` (humanoid hip / shoulder, swing half-angle clamp;
    `twist_half_angle` stored but not enforced by the position-based
    solver)
  Constructors: `Joint::slider` / `Joint::fixed` / `Joint::cone_twist`.
  Example: `cargo run --example constraint_demo` (piston + weld + cone
  twist scenes).

### v0.7 wave 12 — P2P bridge stack

Six new dedicated P2P bridge traits so the engine can plug into
real peer-to-peer stacks (libp2p, WebRTC, custom DHT) without
owning the wire protocol. Each ships with a `Mock*` test
implementation that exercises the contract.

- **`PeerIdentity`** — `peer_id` / `sign(message)` /
  `verify(peer_id, message, signature)` for Ed25519-style
  authenticated peers.
- **`PeerDiscovery`** — `announce(self_addr)` / `known_peers()`
  (returns `PeerAddress { peer_id, multiaddr }`) / `forget(peer_id)`
  for mDNS / DHT bootstrap / manual peer tables.
- **`NatTraversal`** — `gather_candidates() → Vec<IceCandidate>`
  (host / srflx / relay) / `pair_with(remote, candidates)` /
  `punch_hole(remote)` for STUN / TURN / ICE.
- **`DhtProvider`** — `put(key, value)` / `get(key) → Option<Vec<u8>>` /
  `find_peers(key)` for Kademlia-style key-value lookup.
- **`GossipProvider`** — `subscribe(topic)` / `unsubscribe(topic)` /
  `publish(topic, payload)` / `drain_inbox() → Vec<GossipMessage>` /
  `peer_count()` for libp2p `gossipsub`-style fanout.
- **`CrdtSync`** — `local_update(key, value)` / `encode_delta()` /
  `apply_delta(bytes)` / `snapshot()` for CRDT (Automerge / Yjs /
  RGA) state convergence between peers.

Bridge surface is now **27 traits** (= 15 from Wave 0 + 6 from
Wave 11 + 6 from Wave 12).

### v0.7 wave 11 — ALICE-xxx eco-system bridges

Six new `bridge::*` traits so the engine can plug into the rest of
the ALICE Eco-System without picking up the implementing crate as
a hard dependency. Each trait ships with a `MockX` test
implementation that proves the contract is round-trip-safe.

- **`SyncProvider`** (= ALICE-Sync) — `publish(topic, payload)` +
  `poll()` + `subscribed_topics()` for multiplayer / scene-sync
  pub/sub.
- **`CognitiveProvider`** (= ALICE-Cognitive) — `perceive` /
  `decide → CognitiveAction` (Idle / Move / Speak / UseAbility /
  Custom) / `learn(reward)` for NPC AI brains.
- **`MediaCodec`** (= ALICE-Codec) — `encode_frame` / `decode_frame`
  + `format_name` for replay capture and cinematic playback.
- **`LolScriptProvider`** (= ALICE-LOL) — `compile_lol(source) →
  SDF JSON` so DSL authoring drops straight into the engine.
- **`VfxProvider`** (= ALICE-Visual) — `spawn_effect(name, x, y, z)`
  + `active_effect_count()` for higher-level VFX without a hard
  dep on a specific VFX implementation.
- **`AssetCacheProvider`** (= ALICE-Cache) — `get` / `put` /
  `remove` / `len` for streamed / network-loaded resource caching.

Bridge surface is now 21 traits (= 15 from Wave 0 + 6 new),
covering every ALICE-* crate that has a sensible game-engine
integration point.

### v0.7 wave 10 — GUI stack v2 (grid + animation + text input + notify)

Builds on the Wave 9 foundation so the engine's GUI surface
reaches Web / React parity.

- **`grid`** — CSS-Grid style layout solver alongside `flex`.
  `TrackSize::Fixed(px)` / `Fraction(weight)` (CSS `fr` analogue)
  / `Auto` + per-cell `column_span` / `row_span`. Useful for
  dashboards, photo galleries, header bars with named columns.
- **`ui_animation`** — `UiTransition` + 5 `Easing` curves (Linear /
  EaseIn / EaseOut / EaseInOut / Spring). `set_target(value, dur)`
  starts the animation, `tick(dt)` advances it. Mid-flight target
  changes restart the curve from the current value (responsive UI
  feel). Useful for hover / pressed / focused state interpolation.
- **`imgui::text_input`** — single-line text field. `UiInput` gains
  `typed: Vec<char>` + `backspace: bool` so any host input layer
  can forward keystrokes. Records `UiInteraction::TextInputEdited`
  / `TextInputFocused` for game-side handlers.
- **`notify`** — `NotifyCenter` toast / modal queue. `push` adds
  a `Notification` (Info / Success / Warning / Error severities);
  `tick(dt)` auto-dismisses toasts and leaves modals; `active()`
  returns modals first then toasts in arrival order. Capacity caps
  drop the oldest toast but never a modal.

### v0.7 wave 9 — Bevy-style GUI stack

Four new modules + theme bring the engine's UI surface up to
parity with the bevy_ui / bevy_egui / bevy_inspector_egui trio.

- **`flex`** — Flexbox layout solver. `FlexDirection` (Row /
  Column) × `JustifyContent` (Start / Centre / End / SpaceBetween /
  SpaceAround) × `AlignItems` (Start / Centre / End / Stretch) with
  `flex_grow` weight + uniform `gap`. `solve(container, layout,
  children) → Vec<ResolvedChild>` returns per-child rects.

- **`imgui`** — Immediate-mode GUI builder
  (egui / `bevy_egui` style). `UiContext::new(UiInput)` creates a
  per-frame builder, widgets `label` / `button` / `slider` /
  `checkbox` record [`UiCommand`]s for the renderer + interaction
  events for game logic. Auto vertical stacking, hit-testing
  against the cursor + primary press / release flags.

- **`inspector`** — Scene-graph inspector (`bevy_inspector_egui`
  style). `Inspector::rows(&scene)` returns a flat `Vec<InspectorRow>`
  with depth-indented names + per-kind label; `Inspector::detail(
  scene, id)` returns the `(field, value)` pairs for the right-side
  property panel. Read-only by design — mutations route through
  `Editor::apply` so undo/redo stays consistent.

- **`theme`** — `UiTheme` colour palette + spacing scale + font
  size scale, with `dark` / `light` presets (= IDE default / Material
  Design). Stateless, serde round-trippable.

### v0.7 wave 8 — ALICE-SDF feature port

Adopts six high-value features from `ALICE-SDF` so the engine's
in-tree SDF support catches up to the standalone crate without
adding it as a dependency.

- **Adaptive Marching Cubes** (`sdf::adaptive_marching_cubes`) —
  octree refinement on top of the existing MC: skips empty octants,
  subdivides only where corner-vs-centre error exceeds a threshold,
  falls back to the standard `marching_cubes` at the leaves.
- **Dual Contouring** (`sdf::dual_contouring`) — one vertex per cell
  placed at the average surface intersection plus stitched quads
  across sign-changing edges. Sharp edges survive better than MC;
  QEF refinement omitted for now (= still close to ALICE-SDF
  output for organic shapes).
- **`sdf::shell_offset`** — `(eval(p) + offset).abs() - thickness/2`
  turns any SDF into a hollow shell, with optional offset shift.
- **Volume / Surface Area Monte Carlo** (`sdf::volume_monte_carlo`,
  `sdf::surface_area_monte_carlo`) — deterministic PRNG samples a
  bounding box and returns `(estimate, standard_error)`. Validated
  against the unit sphere's known volume and area.
- **`sdf2d` module** — 5 primitives (`Circle`, `Box`, `RoundedBox`,
  `Segment`, `Triangle`) + 4 boolean ops (Union / Intersect /
  Subtract / SmoothUnion) + `sample_grid` for icon / font /
  UI-mask rasterisation.
- **`heatmap` module** — `heatmap_slice(node, axis, depth, ...)`
  samples an SDF slice and returns an RGBA buffer ready to upload
  to `Rgba8Unorm`. Four scientific colormaps (CoolWarm, Binary,
  Viridis, Magma).

### v0.7 wave 7 — end-to-end demos + Android app

- **virtual_shadow caster demo** —
  `templates/virtual_shadow_demo.rs` allocates one atlas page,
  runs a depth-only render pass through
  `VirtualShadowGpu::render_caster_to_page` writing a fixed depth
  (`0.42`), then reads back the centre + a foreign-page texel to
  prove the viewport restriction. Verified on Apple M3
  (= inside `0.420`, outside `1.000`).

- **Android Studio sample app** — new `android/` directory ships
  a complete minimum-viable Android Studio project:
  `settings.gradle.kts` / `build.gradle.kts` / `app/build.gradle.kts`
  + `AndroidManifest.xml` + `AliceGameEngine.java` (JNI wrapper,
  `AutoCloseable`, opaque `long` handle, phase constants) +
  `MainActivity.java` (Choreographer-driven tick + touch
  forwarding). Native build path documented in `android/README.md`
  (= `cargo-ndk` 一発で `libalice_game_engine.so` ⇒
  `jniLibs/arm64-v8a/`).

- **Cubemap sky GPU demo** —
  `templates/cubemap_sky_demo.rs` runs
  `render_sky_to_faces` on a real GPU, reads back all 6 faces of
  the `Rgba16Float` cube texture, decodes the centre half-float
  texel per face, and asserts the procedural sky filled the texture
  with the expected gradient. Verified on Apple M3 (= +Y zenith
  blue / -Y horizon warm / sides smooth gradient).

- **`virtual_shadow` atlas texture usage** — `COPY_SRC` added so
  shadow data can be read back to host memory (= unit tests, screen
  capture, debug overlays).

### v0.7 wave 6 — driver demos + JNI bridge

Closes the four residual items from Wave 5.

- **TLAS bottom-up dispatch demo** —
  `templates/gpu_bvh_refit_demo.rs` builds a small BVH, dispatches
  `gpu_bvh_interior_refit_compute` once per
  `Bvh::levels_bottom_up()` slot, reads back the root node, and
  asserts the GPU-refit AABB matches the CPU-built scene bounds.
  Verified on Apple M3 (= 3 levels, root bounds = scene bounds).

- **`virtual_shadow::render_caster_to_page`** — one-page depth
  render driver: opens a depth-only render pass scoped to the
  page's atlas viewport (= `set_viewport` with `page_uv_offset`),
  invokes the caller's `draw_callback`, and submits. The smallest
  driver an engine needs to fill exactly the dirty pages of a
  virtual shadow map.

- **`Humanoid::bind_from_vrm(&VrmExtract)`** — bridges the VRM
  camelCase naming (`leftUpperArm`) to the engine's snake_case
  [`HumanoidBone`] (`left_upper_arm`) and bulk-binds every known
  bone in one call. Unknown bones are skipped (= forward
  compatible).

- **Android JNI wrapper scaffold** — `mobile::AliceGameEngineHandle`
  + `alice_ge_create` / `alice_ge_tick` / `alice_ge_touch` /
  `alice_ge_destroy` (`#[unsafe(no_mangle)] extern "C"`) so the
  Java side can store an opaque pointer + dispatch frame ticks +
  touch events. ABI is now frozen; real method bodies land in a
  follow-up PR without breaking the Java wrapper.

### v0.7 wave 5 — production drivers (residual gap)

Closes the four "scaffold あり / driver 未着" items left over from
Wave 4.

- **TLAS interior refit driver** —
  `Bvh::levels_bottom_up()` returns node indices grouped by tree
  level (leaves first, root last) for the GPU dispatcher.
  `shader::GPU_BVH_INTERIOR_REFIT_COMPUTE_WGSL` unions each interior
  node's children bounds (one workgroup invocation per node in the
  current level). The bottom-up dispatch sequence is now a one-loop
  driver: iterate over `levels_bottom_up()` and dispatch
  `gpu_bvh_interior_refit_compute` per level.

- **Cubemap sky render driver** —
  `CubemapCaptureTargets::render_sky_to_faces(device, queue,
  atmosphere)` builds a full wgpu render pipeline against
  `CUBEMAP_SKY_FRAGMENT_WGSL`, allocates per-face uniform buffers
  (= inverse view-projection + sun + horizon / zenith colors), and
  submits a fullscreen-triangle draw into every face. The probe is
  now populated with the procedural sky in one call.

- **VRM full extract** — `parse_vrm_full(json) → VrmExtract` now
  returns the full humanoid bone binding list and expression preset
  weights, not just the meta block. Includes `VrmBoneBinding`
  (`bone_name`, `node_index`) + `VrmExpressionBinding` (`preset`,
  `weight`) and gracefully handles VRMs that omit either section.

- **`virtual_shadow` GPU pipeline** —
  `VirtualShadowGpu::new(device, atlas_pages_per_side, page_size)`
  allocates a `Depth32Float` atlas texture + view + comparison
  sampler (= the residency that the existing
  `VirtualShadowMap` page table indexes into). `page_uv_offset`
  converts a `PhysicalPageHandle` into the atlas UV offset shader
  code reads from.

### v0.7 wave 4 — evaluation report follow-through

Closes the 10 residual items from the v0.6 evaluation report (E.
弱み / 残課題). Each item now ships at least scaffold + tests + doc.

- **Cubemap GPU full render** —
  `shader::CUBEMAP_SKY_FRAGMENT_WGSL` (naga-validated) renders the
  procedural sky into a cubemap face; combine with
  `env_probe::CubemapCaptureTargets` for a real dynamic IBL probe.

- **iOS / Android library binaries** — `Cargo.toml` `[lib]
  crate-type = ["rlib", "staticlib", "cdylib"]` now produces
  `.rlib` + `.a` + `.dylib` from one `cargo build`. Verified locally
  on macOS aarch64 (= 3 artefacts in `target/release/`).

- **Editor websocket UI** — `templates/editor_ui/index.html`
  ships an embedded browser editor (connect / hello / snapshot /
  undo / redo / translate / rename / hide) and
  `editor_server_demo` serves it at `http://127.0.0.1:8088/`.

- **GPU compute light culling demo** —
  `templates/gpu_compute_light_culling_demo.rs` dispatches the
  tiled-light compute shader on a real GPU device. Verified on Apple
  M3 (4 lights, 240/240 tiles covered).

- **TLAS/BLAS GPU scaffold** — `shader::GPU_BVH_REFIT_COMPUTE_WGSL`
  (naga-validated) implements the leaf-refit pass for the existing
  CPU-built `Bvh`; the bottom-up interior refit driver lands in a
  follow-up PR.

- **Asset I/O extension** — `asset::parse_vrm_json` (VRM 1.x meta
  block), `asset::parse_fbx_header` (Kaydara FBX binary magic +
  version), `asset::is_usda` (`#usda` ASCII recogniser) — scaffold
  recognisers so importers can branch.

- **Multi-platform CI** — `.github/workflows/ci.yml` test job runs
  on a `[ubuntu-latest, macos-latest, windows-latest]` matrix.

- **Procedural animation** — new `cloth` module (mass-spring grid
  with Verlet + length constraint + wind) and `soft_body` module
  (3-axis lattice with pin support). 13 unit tests.

- **TUTORIAL.md** — five-step getting-started covering install /
  LLM NPC / SDF+mesh scene / RPG battle / editor websocket.

- **`virtual_shadow` scaffold** — `VirtualShadowMap` page allocator
  (UE5-style) with `allocate` / `release` / `lookup`. 6 unit tests.

### v0.7 wave 3 — 4-region production wiring

- **Editor websocket server demo** — new `editor_server` feature
  (axum 0.7 + tokio macros / rt-multi-thread) + `templates/
  editor_server_demo.rs` runs an axum server on `127.0.0.1:8088`
  with `/ws` handling JSON-encoded `EditorClientMessage` frames via
  `dispatch_client_message`. Shared scene + editor behind
  `tokio::sync::Mutex` for multi-client safety.

- **Cubemap GPU render driver** —
  `CubemapCaptureTargets::clear_all_faces(device, queue, color)`:
  smallest "render driver" that submits 6 clear-only passes against
  the 6 face views, proving the wiring before the engine plugs in
  its real deferred render. Production code swaps the clear for the
  6-face deferred render.

- **Mobile CI smoke** — `.github/workflows/ci.yml` gains a new
  `mobile-build` job on `macos-latest` that runs `cargo check
  --target aarch64-apple-ios` + `--target aarch64-linux-android`
  with `--no-default-features` so iOS / Android cross builds are
  validated on every PR.

- **GPU compute headless demo** — new `templates/gpu_compute_demo.rs`
  drives the DDGI update compute shader end-to-end on a real GPU
  via pollster + wgpu Instance/Adapter/Device, dispatches via
  `dispatch_compute_once`-style flow, reads back the irradiance
  buffer, and verifies the mean matches the input samples (= 0.7).
  Verified on Apple M3 (Metal). Falls back to a graceful skip
  message when no compatible adapter is available.

### v0.7 wave 2 — 4-region depth

- **`editor`** undo/redo wired through — `Editor::undo` /
  `Editor::redo` mutate the scene, push/pop the inverse on the
  appropriate stack, and a new `apply` clears the redo stack
  per the standard editor convention. New `EditorCommand::RemoveNode`
  variant + post-execute `patch_inverse_for_add` so `AddNode` /
  `RemoveNode` round-trip with the correct node id.

- **`editor`** websocket / MCP protocol enums — `EditorClientMessage`
  (`Hello` / `Apply` / `Undo` / `Redo` / `Snapshot`),
  `EditorServerMessage` (`Welcome` / `Outcome` / `Snapshot` /
  `Error`), `EDITOR_PROTOCOL_VERSION = 1`, `dispatch_client_message`
  pure-function dispatcher. Ready for both axum websocket transports
  and MCP `tools/call` adapters in a follow-up PR.

- **`gpu`** compute helpers — `GpuContext::create_storage_buffer`,
  `create_empty_storage_buffer`, and `dispatch_compute_once`
  (one-shot compute submit + readback) so module-side `*Gpu`
  pipelines can be exercised from unit tests / offline tools without
  every caller duplicating the encoder + map_async boilerplate.

- **`env_probe`** GPU cubemap targets — `CubemapFaceCamera` +
  `cubemap_face_cameras(position, near, far)` + (under feature
  `gpu`) `CubemapCaptureTargets::new(device, ...)` which allocates a
  6-layer `Rgba16Float` cube texture, the cube-array view, and one
  face-level view per face so the existing deferred renderer can run
  6 face passes into the same target.

- **`mobile`** target descriptor — `MobileTarget` enum (Ios /
  Android / Other), `MobileTarget::current()` resolves at compile
  time from `cfg(target_os)`, plus `mobile_build_hints()` and stub
  `android` / `ios` submodules (= placeholders for platform glue).

### v0.7 wave 1 — 4-region expansion

- **GPU compute shaders** — `LIGHT_CULLING_COMPUTE_WGSL`
  + `DDGI_UPDATE_COMPUTE_WGSL` const (naga-validated, builtin cache
  10 → 12). Per-module wgpu pipeline scaffolds:
  `light_culling::TiledLightCullerGpu` (= 4-binding compute pipeline,
  `workgroup_count_x(light_count)` helper) and
  `ddgi::DdgiVolumeGpu` (= 3-binding compute pipeline, one workgroup
  per probe, 8×8 threads). CPU implementations stay as the reference
  / fallback.

- **`env_probe`** new helpers — `cubemap_face_views(position)` returns
  the six standard cube-face view matrices; `capture_sky_to_cubemap(
  resolution, atmosphere)` rasterises [`sky::sky_color`] into a fresh
  [`Cubemap`] so probes can be seeded without the full GPU 6-face
  capture pass landing yet.

- **`mobile`** new module — `TouchPhase` / `TouchEvent` / `ScreenMetrics`
  + `TouchCamera` (= single-finger orbit, two-finger pinch zoom,
  device-independent units). Renderer-agnostic so iOS / Android / web
  shells can forward platform touches with no extra glue.

- **`editor`** new module — `EditorCommand` (Add / Hide / Show /
  Translate / SetScale / SetRotation / Rename), `Editor::apply`
  mutates a [`SceneGraph`] and records the inverse on
  `EditorHistory` (capacity-bounded). JSON round-trip ready, designed
  to be the receiving end of the MCP server's `editor.apply` tool and
  a future websocket transport for browser editors.

- **`gpu_bvh`** — CPU-side BVH + Morton-code radix sort. `Aabb`
  (union / centre / surface_area), `morton3_unit(xyz)` (30-bit
  Z-curve code in unit cube), `radix_sort_u32_pairs` (stable LSD
  3-pass × 11-bit, payload-preserving), `Bvh::build(aabbs, leaf_size)`
  (Morton-sorted, median-split top-down build). Same radix sort
  feeds Gaussian splat tile bins and particle / collision broadphase.
  Demo: `cargo run --example gpu_bvh_demo`.

- **`ddgi`** — Dynamic Diffuse Global Illumination (Majercik et al.
  2019). `DdgiConfig` (grid / spacing / resolution / hysteresis),
  `DdgiVolume::update_probe_irradiance` (low-pass filter blending),
  `sample_irradiance(world, dir)` (trilinear 8-probe blend, octahedral
  direction lookup, world-volume bounds check). `dir_to_oct` /
  `oct_to_dir` octahedral encoding utilities. Demo:
  `cargo run --example ddgi_demo` (warm-interior / cool-exterior
  probe volume queried at four world positions).

- **`gaussian_splat`** — 3D Gaussian Splatting (Kerbl et al. 2023)
  CPU data + per-frame projection. `Splat` (position / anisotropic
  scale / rotation / colour / opacity / SH band-1 coefficients),
  `Splat::isotropic` constructor, `GaussianCloud::prepare_frame(view,
  projection)` does frustum culling + screen-space projection +
  back-to-front depth sort. `evaluate_sh_band1(coeffs, view_dir)` for
  view-dependent colour. Tile-blending shader lives in a future PR.
  Demo: `cargo run --example gaussian_splat_demo` (200 random splats
  projected and depth-sorted in <1 ms).

- **`humanoid`** — VRM 1.0-style humanoid skeleton mapping +
  expression channels. `HumanoidBone` enum (25 canonical VRM bones),
  `Humanoid::bind` / `get` / `meets_required` / `missing_required`,
  `ExpressionChannel` (5 visemes + 7 emotion presets + `Custom`),
  `ExpressionSet::set` / `weight` / `remove` / `reset` /
  `set_visemes` lip-sync helper. Renderer-agnostic — owns mappings
  and weights, not transforms. Demo:
  `cargo run --example humanoid_demo` (4-frame lip-sync + blink +
  happy overlay).

- **`volumetric_clouds`** — Raymarched volumetric cloud module
  (Horizon Zero Dawn / Wicked Engine style). `VolumetricCloudConfig`
  (coverage / density / wind / altitude band / step count / sun),
  `cloud_density(world_pos, time, config)` (FBM base + detail erosion
  + height window), `march_cloud_ray(origin, dir, time, config) →
  CloudRayResult` (Beer's-law transmittance + HG scattering). Demo:
  `cargo run --example volumetric_clouds_demo` (ASCII sky at three
  coverage levels + wind advection).

- **`hair`** — CPU hair / grass strand simulator. `HairConfig`
  (segments / length / wind_strength / gravity / stiffness / LOD),
  `HairSystem::add_strand` + `simulate(dt, wind)` does Verlet
  integration with a length constraint, per-strand seed jitter, and
  LOD cutoff. Demo: `cargo run --example hair_demo` (64 grass blades
  swayed by a constant wind).

- **`env_probe`** — Environment probe / IBL data + prefilter math.
  `EnvProbeData` (scene-graph payload), `Cubemap` (6-face RGBA32F,
  direction sampling, bilinear), `prefilter_irradiance` (cosine
  convolution → diffuse IBL), `prefilter_radiance` (Phong-style mip
  chain → specular IBL split-sum), `PrefilteredEnvProbe` bundle.
  `NodeKind::EnvProbe` + `SceneGraph::env_probes()` collector. WGSL
  `IBL_LOOKUP_WGSL` helper for shader-side cubemap sampling, naga-
  validated. Built-in shader cache now reports 10 entries.

- **`ocean`** — Tessendorf-style FFT ocean simulator (Wicked-inspired
  `wiOcean`). `OceanConfig` (grid_size / patch_size / wind / amplitude /
  gravity), `OceanSimulator::simulate(time) → OceanFrame` with height
  field + central-difference normals. **No external dependencies**:
  ships a self-contained Cooley-Tukey radix-2 IFFT, Box-Muller
  Gaussian noise from a deterministic 64-bit hash. 32×32 grid runs in
  ~0.5 ms / frame on Apple Silicon. Demo:
  `cargo run --example ocean_demo` (ASCII heightmap at four time steps).

- **`light_culling`** (feature `gpu`) — Tiled (Forward+ style) CPU-side
  light culler. `LightCullingConfig` (tile_size / max_lights_per_tile),
  `TiledLightCuller::cull(lights, view, proj) → TileLightList` returns
  per-tile point/spot light index lists + a separate directional list.
  Per-tile overflow drops the farthest lights (distance priority).
  Demo: `cargo run --example light_culling_demo --features gpu` (128
  point lights + 1 directional over 1920×1080, 16-px tiles).

- **`shader`** — `TILED_LIGHTING_FRAGMENT_WGSL` const added (storage-
  buffer-based per-tile light index lookup, naga-validated). Built-in
  shader cache now reports 9 entries.

- **`jobs`** — Fork-join job system (Wicked-inspired `wiJobSystem`).
  `JobContext` (Mutex+Condvar pending counter, optional parent for
  nesting), `JobArgs` (job_index / group_id / group_index), top-level
  `execute` / `dispatch` / `wait`. Dedicated `rayon::ThreadPool`
  (isolated from the global pool used by `sdf::marching_cubes_parallel`).
  Panic-safe via RAII `JobGuard` so `wait` always unblocks. Example:
  `cargo run --example job_system_demo` (256 fork-joined "particle" jobs
  + 2 "asset" jobs, single parent `wait` covers both stages).

### Scene graph + renderer integration

- `NodeKind::Decal(DecalData)` — decals are first-class scene graph
  nodes; their OBB extents come from `local_transform.scale`.
- `SceneGraph::decals()` collector + `local_aabb` arm for the unit OBB.
- `FrameData` gains `decal_draws: Vec<DecalDraw>`, collected per frame
  with pre-computed inverse world matrices for shader use.
- `RenderPass::Decal` inserted between `GBuffer` and `DeferredLighting`
  in `Renderer::active_passes()`; `DrawStats.decal_nodes`,
  `PipelineState.decal_pass_enabled` added.

### Shaders

- `DECAL_VERTEX_WGSL` + `DECAL_FRAGMENT_WGSL` — depth-reconstruct OBB
  projection with branchless blend mode dispatch matching
  `DecalBlendMode::shader_id`. Both naga-validated in unit tests. Built-
  in shader cache now reports 8 entries (was 6).

### Example

- `cargo run --example decal_demo` — five decals (bullet hole, blood,
  graffiti, sign, glowing rune) projected onto a wall mesh and SDF
  floor, headless.

## [0.6.0] - 2026-06-11

Major content release: turn-based RPG, 3D action combat, hierarchical
pathfinding, HD-2D post-process, multiplayer scaffolding, and 14 no-code
event commands. 793 lib tests / 1,007 with `full` features.

### New modules

- **`battle`** — `TurnBattleRunner` with speed-ordered execution,
  `Attack` / `Defend` / `Flee` / `UseAbility` / `Move`, `BattleAi` trait,
  `RandomAi`, `GridCell` + Chebyshev attack range.
- **`action_combat`** — `Hitbox` / `Hurtbox` (sphere + capsule),
  `resolve_hits`, `ComboSystem` with input window, `LockOn` cone,
  `HitStop`.
- **`hd2d_postfx`** — `Sprite3D` billboard + WGSL templates for
  `hd2d_sprite`, `ssgi`, `smaa`. naga-validated + GPU-loaded on Mac Metal.

### Scripting

- 13 serializable `EventCommand`s — `Message` / `ChangeAttr` / `Wait` /
  `Branch` / `GiveItem` / `BeginBattle` / `Choice` / `SetVar` / `IfVar` /
  `SetSwitch` / `HasItem` / `TakeItem` / `MapTransition`.
- 5 advanced flow commands — `Cutscene` (multi-line dialogue),
  `Parallel`, `Repeat`, `LoopUntil`, `LlmDialogue` (LLM-backed NPC).
- `EventScriptDef` JSON form for no-code editors / hot reload.

### Bridge

- New `bridge::WorldProvider` trait — themed-world plug-in (zones,
  weather, camera, teleport).
- `network::LoopbackTransport` implementing `bridge::NetworkTransport`,
  driving the new `multiplayer_battle` example.

### Existing modules — additions

- **`animation`** — 2-bone analytic IK solver, `BlendTree1D`.
- **`audio`** — `MusicTrack` cross-fade BGM controller, `ReverbZone` with
  4 presets (outdoor / room / cavern / underwater).
- **`navmesh`** — Grid → NavMesh auto-generation, hierarchical A* with
  cluster planning.
- **`particle`** — Curl-noise force field, `TrailEmitter` with `max_len`.

### Examples

- `rpg` — full opening / choice / battle / reward flow.
- `multiplayer_battle` — two peers over `LoopbackTransport` driving one
  `TurnBattleRunner`.
- `visual_novel` — branching dialogue + LLM line + heroine route, all
  via `EventScript`.
- `fps_combat` — `LockOn` + hitscan + `HitStop`.
- `platformer_action` — sword hitbox + dash trail (Curl Noise) (requires
  `--features particles`).

### Quality

- clippy (pedantic + nursery): 0 lib warnings (style allows justified at
  the `lib.rs` top, see comments).
- 793 unit tests / 1,007 with `full` features. CI 7-in-a-row green.
- 6 WGSL shaders / pipelines verified end-to-end on Mac Metal (offscreen
  triangle readback passes — green pixel confirmed).
- New `tokio` dependency only in downstream consumers needing
  `AliceSyncTransport`; the engine crate itself stays dep-free of
  the ALICE-xxx stack.

## [0.5.0] - 2026-03-28

Initial public release. 31 modules, 17,932 lines, 688 tests.
Dual licensed: MIT + Commercial.

### Core
- **ECS** (1,781 lines, 107 tests): Generational arena, sparse-set ComponentStore, World, Scene, spatial hash grid broadphase
- **Scene Graph** (1,277 lines, 43 tests): Mesh+SDF hybrid NodeKind, hierarchical TRS, AABB3, frustum culling, reparenting, descendants query
- **Engine** (354 lines, 11 tests): System trait, fixed timestep (60Hz), interpolation alpha, configurable max steps
- **Math** (776 lines, 30 tests): Vec2/3/4, Mat4, Quat, Color, perspective + orthographic projection, sRGB conversion

### Rendering
- **GPU** (521 lines, 10 tests): wgpu Device/Queue/Surface, render_mesh(), render_clear(), vertex/index/uniform buffer creation, RGBA8 texture upload
- **Renderer** (773 lines, 25 tests): Deferred GBuffer (5 attachments), RenderGraph (Kahn topological sort), DebugRenderer (wireframe/AABB)
- **Shader** (439 lines, 15 tests): ShaderCache, 5 built-in WGSL shaders (GBuffer vertex/fragment, SDF raymarch, fullscreen vertex, deferred lighting)
- **Render Pipeline** (354 lines, 13 tests): FrameData scene extraction, MvpUniforms, MaterialUniforms, PipelineState
- **GPU Mesh** (280 lines, 9 tests): GpuMeshDesc, VertexLayout (Position3F/Normal3F/Uv2F/Color4F/Tangent4F), DrawCommand/DrawQueue with material sort
- **Texture** (400 lines, 18 tests): TextureAsset, mipmap level calculation, checkerboard generator, GpuTextureDesc, SamplerDesc
- **LOD** (264 lines, 13 tests): LodGroup, screen coverage calculation, batch LOD selection, pixel-size culling

### SDF
- **SDF** (1,112 lines, 37 tests): 7 primitives (Sphere/Box/Capsule/Cylinder/Torus/Plane/Cone), 6 boolean ops (Union/Intersection/Subtraction + smooth variants), full TRS transform, standard Marching Cubes (256-entry edge+triangle tables), sphere tracing, SDF sphere collider, normal estimation

### Physics
- **Physics3D** (696 lines, 32 tests): RigidBody with mass/restitution/friction, semi-implicit Euler integration, sweep-and-prune broadphase O(n log n), impulse-based contact resolution, linear/angular damping, sleeping/wake, torque, gravity
- **Collision** (333 lines, 10 tests): GJK convex intersection (ConvexHull/ConvexSphere), Indeterminate result for non-convergence, SDF-mesh hybrid narrowphase

### Audio
- **Audio** (975 lines, 39 tests): AudioBus with ping-pong buffer effects chain, static dispatch Effect enum (Attenuate/LowPass/HighPass/BandPass/Reverb), HRTF processor (ITD+ILD), AudioSource with PCM buffer playback + looping, spatial equal-power panning, AudioEngine.render() full pipeline, WAV export

### Animation
- **Animation** (650 lines, 32 tests): Keyframe (Linear/Step/CubicBezier interpolation), Track with binary search evaluation, AnimationClip with looping, AnimationPlayer (play/pause/stop/speed), StateMachine with timed transitions and blend factor

### Input
- **Input** (587 lines, 16 tests): Keyboard/Mouse/Gamepad with just_pressed/just_released tracking, ActionMap name-based binding, AxisSource (KeyPair/GamepadAxis/MouseX/Y/Scroll)
- **Window** (263 lines, 15 tests): WindowConfig, winit KeyCode mapping, FrameTimer with 60-sample smoothed FPS

### UI
- **UI** (951 lines, 30 tests): Retained-mode widget tree (Panel/Button/Label/TextInput/Checkbox/Slider/Image/ScrollArea/DropdownList/ProgressBar), Measure/Arrange layout (Vertical+Horizontal), UiTheme, FocusManager (tab order), message passing (BubbleUp/Direct routing), hit testing

### Navigation
- **NavMesh** (654 lines, 21 tests): NavMesh (vertex/triangle/neighbor), A* pathfinding, NavAgent waypoint following, SDF dynamic obstacle avoidance, crowd separation (RVO-style)

### Gameplay
- **Ability** (501 lines, 16 tests): Gameplay Ability System (UE5 GAS inspired) — Attribute/AttributeSet, GameplayEffect (Instant/Duration/Infinite), AttributeModifier (flat/multiply), Ability with cooldown+cost, AbilitySystem tick/activate
- **Scripting** (549 lines, 24 tests): EventBus (publish/subscribe), Timer/TimerManager (OneShot/Repeating), ScriptVars (typed variable storage)

### 2D
- **Scene2D** (532 lines, 21 tests): Sprite2D, TileMap (grid + world coordinate conversion), Aabb2, Body2D, 2D AABB collision detection, SDF2D circle test, Scene2D z-order rendering

### Asset & Import
- **Asset** (333 lines, 13 tests): OBJ parser (v/f with fan triangulation), glTF binary header parser, SDF JSON loader, asset type detection by extension
- **Import** (409 lines, 17 tests): Unity YAML scene parser (GameObject/Transform/MeshRenderer/Camera/Light), UE5 .uasset header parser, automatic format detection, unity_to_node() ALICE conversion
- **Resource** (309 lines, 12 tests): Async resource manager with Pending/Ready/Failed states, ref counting, path-based lookup

### Infrastructure
- **App** (715 lines, 13 tests): `run_windowed()` with winit ApplicationHandler (keyboard/mouse/resize), HeadlessRunner for testing, WAV audio export
- **Camera Controller** (322 lines, 19 tests): FPS camera (WASD+mouse look, pitch clamp), Orbit camera (rotate/zoom/pan)
- **Query** (293 lines, 11 tests): Typed ECS queries (query2/query3), filter_with/filter_without, SystemScheduler (priority-ordered)
- **Particle** (432 lines, 16 tests): ParticleEmitter (Point/Sphere/Box/Cone shapes), gravity, color/size interpolation, LCG RNG
