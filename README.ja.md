# ALICE-GameEngine

メッシュ + SDF ハイブリッドゲームエンジン (Rust 製)。42 モジュール、**744 テスト** (lib) / 24 (doc)、wgpu deferred renderer (Vulkan/Metal/DX12/WebGPU)、**ターン制バトル + ノーコードイベントスクリプト** で RPG が作れます。

[English](README.md)

## 主な機能

- **ハイブリッドシーングラフ** — メッシュと SDF ボリュームを同じツリーに混在
- **wgpu Deferred レンダラー** — GBuffer、RenderGraph、debug overlay
- **Verlet 物理** + sweep-and-prune 広域 + SDF CCD
- **HRTF オーディオ** バス効果と空間パン
- **ターン制 RPG ランタイム** — `TurnBattleRunner` + 13 種の `EventCommand`
  (Choice/SetVar/IfVar/Switch/HasItem/TakeItem/MapTransition/Cutscene/Parallel/Repeat/LoopUntil/LlmDialogue/...)
- **`bridge::*` trait** — ALICE-SDF, ALICE-Physics, ALICE-Audio, ALICE-Text
  等を差し替え可能な抽象境界で接続
- **XR レイヤー** (Pure-Rust、openxr 依存なし) MockProvider + StereoWindow

## クイックスタート (5 行)

```rust
use alice_game_engine::easy::*;

let mut game = GameBuilder::new("My Game").build();
game.add_camera();
game.add_cube(0.0, 1.0, -5.0);
game.add_sphere_sdf(3.0, 0.0, 0.0, 1.0);
game.add_light(0.0, 10.0, 0.0);
game.run_headless(300);
```

全型を一行でインポート:

```rust
use alice_game_engine::prelude::*;
```

## ウィンドウ表示

```bash
cargo run --example spinning_cube --features full
```

wgpu でカラフルなキューブが回転するウィンドウが開きます。Escで終了。

## ターン制 RPG

RPG スターターテンプレートを example として登録済み (`templates/rpg.rs`):

```bash
cargo run --example rpg
```

実行結果 (抜粋):

```
Elder: Welcome, traveler. A slime has made the cave its home.
CHOICE: Will you help us?
  (1) Accept  (2) Decline
> Accept
[switch quest_active = true]
[Battle begins: cave_slime]

=== Battle: Hero vs Slime ===
  Hero attacks Slime for 13 damage. (12 HP left)
  Slime attacks Hero for 2 damage. (58 HP left)
  Hero attacks Slime for 13 damage. (0 HP left)
  Slime is defeated!

Elder: Take this potion.
[has_item potion >= 2 ? true]
```

テンプレートは 3 層を組み合わせます:

1. **`battle::TurnBattleRunner`** — 速度順ターンループ。`Attack /
   UseAbility / Defend / Flee` アクション、`BattleAi` trait、デフォルトの
   `RandomAi` を用意
2. **`scripting::EventScript`** — `EventCommand` のシーケンス。13 種類:
   `Message`, `ChangeAttr`, `Wait`, `Branch`, `GiveItem`, `BeginBattle`,
   `Choice`, `SetVar`, `IfVar`, `SetSwitch`, `HasItem`, `TakeItem`,
   `MapTransition`、加えて高度フロー制御 `Cutscene`, `Parallel`,
   `Repeat`, `LoopUntil`, `LlmDialogue` (LLM 連動 NPC 会話)
3. **`ability::AbilitySystem`** — UE5 GAS インスパイアの属性、効果、
   クールダウン

世界 (ステージ) はエンジン外から `EngineContext::set_world_provider` +
`bridge::WorldProvider` trait 経由で差し込みます。1 度実装すれば、戦闘 +
イベントランタイムを任意の世界観で再利用できます。

```rust
use alice_game_engine::app::{run_windowed, AppCallbacks};
use alice_game_engine::engine::{EngineConfig, EngineContext};
use alice_game_engine::math::{Quat, Vec3};
use alice_game_engine::scene_graph::*;
use alice_game_engine::window::WindowConfig;

struct MyGame;

impl AppCallbacks for MyGame {
    fn init(&mut self, ctx: &mut EngineContext) {
        ctx.scene.add(Node::new("camera", NodeKind::Camera(CameraData::default())));
        ctx.scene.add(Node::new("cube", NodeKind::Mesh(MeshData::default())));
        ctx.scene.add(Node::new("sphere", NodeKind::Sdf(SdfData {
            sdf_json: r#"{"Primitive":{"Sphere":{"radius":1.0}}}"#.to_string(),
            half_extents: Vec3::ONE,
            generate_collider: true,
        })));
    }

    fn update(&mut self, ctx: &mut EngineContext, _dt: f32) {
        let t = ctx.time.total_seconds as f32;
        if let Some(node) = ctx.scene.get_mut(NodeId(1)) {
            node.local_transform.rotation = Quat::from_axis_angle(Vec3::Y, t);
        }
    }
}

fn main() {
    run_windowed(WindowConfig::default(), EngineConfig::default(), Box::new(MyGame)).unwrap();
}
```

## アーキテクチャ

```
                    +-----------+
                    |  app.rs   |  winit イベントループ + wgpu 描画
                    +-----+-----+
                          |
                    +-----+-----+
                    | engine.rs |  System trait、固定タイムステップ (60Hz物理)
                    +-----+-----+
                          |
        +---------+-------+-------+---------+
        |         |       |       |         |
   scene_graph  ecs   physics3d  audio   input
   (メッシュ+SDF) (SoA) (Verlet) (HRTF) (ActionMap)
        |                 |
   +----+----+      broadphase
   |         |      (Sweep-and-Prune)
 renderer   sdf         |
 (wgpu)   (MC+Rayon)   fix128
                        (128bit精度)
```

## モジュール一覧 (42)

| モジュール | 行数 | テスト | 説明 |
|-----------|-----:|------:|------|
| ecs | 1,872 | 107 | SoA スパースセット ECS、空間ハッシュグリッド broadphase |
| scene_graph | 1,277 | 43 | メッシュ+SDFハイブリッドノードツリー、AABB3、フラスタムカリング |
| sdf | 1,243 | 39 | 7プリミティブ、6ブーリアン演算、正規MC (256テーブル)、Rayon並列MC、球トレース |
| audio | 975 | 39 | バスエフェクト (ピンポン)、HRTF、PCM再生、空間パンニング、WAVエクスポート |
| ui | 951 | 30 | 保持モードUI、水平/垂直レイアウト、フォーカス管理、テーマ |
| physics3d | 815 | 36 | ベルレ積分、SAP broadphase、インパルスソルバー、SDF CCD、ダンピング、スリープ |
| math | 776 | 30 | Vec2/3/4、Mat4、Quat、Color、透視/正射影投影 |
| renderer | 773 | 25 | ディファードGBuffer、RenderGraph (Kahnトポソート)、DebugRenderer |
| app | 715 | 13 | `run_windowed()` (winit+wgpu)、`HeadlessRunner`、WAVエクスポート |
| navmesh | 654 | 21 | NavMesh、A*経路探索、SDF動的回避、群衆分離 (RVO) |
| animation | 650 | 32 | キーフレーム (Linear/Step/Cubic)、トラック、クリップ、ステートマシン |
| input | 587 | 16 | キーボード/マウス/ゲームパッド、ActionMap、軸バインディング |
| scripting | 1,560 | 73 | EventBus (Pub/Sub)、Timer/TimerManager、ScriptVars、13 種の EventCommand (Message/Choice/SetVar/IfVar/SetSwitch/HasItem/TakeItem/MapTransition/BeginBattle/Wait/Branch/ChangeAttr/GiveItem) + 高度フロー (Cutscene/Parallel/Repeat/LoopUntil/LlmDialogue) + EventScript ランナー |
| scene2d | 532 | 21 | Sprite2D、TileMap、Aabb2、Body2D、Physics2D、Zオーダー |
| gpu | 521 | 10 | wgpu Device/Queue/Surface、render_mesh()、テクスチャアップロード |
| ability | 501 | 16 | ゲームプレイアビリティシステム (UE5 GAS風): 属性、エフェクト、クールダウン |
| battle | 660 | 15 | ターン制バトルランナー、Battler/Party/BattleAction、Attack/Defend/Flee/UseAbility、BattleAi trait + RandomAi、速度順実行 |
| shader | 439 | 15 | ShaderCache、5つのビルトインWGSLシェーダー |
| particle | 432 | 16 | CPUエミッター、マルチシェイプ (Point/Sphere/Box/Cone)、重力 |
| import | 409 | 17 | Unity YAMLシーンパーサー、UE5 .uassetヘッダー、フォーマット検出 |
| texture | 400 | 18 | TextureAsset、ミップマップ、チェッカーボード、GpuTextureDesc |
| fix128 | 353 | 19 | 128bit固定小数点 (i128, 40分数ビット)、Fix128Vec3、長時間精度 |
| render_pipeline | 354 | 13 | FrameData抽出、MvpUniforms、MaterialUniforms、PipelineState |
| engine | 354 | 11 | ゲームループ、System trait、固定タイムステップ、補間アルファ |
| asset | 336 | 13 | OBJパーサー、glTFヘッダー、SDF JSONローダー |
| collision | 333 | 10 | GJK凸衝突判定、SDF-メッシュハイブリッドnarrowphase |
| camera_controller | 322 | 19 | FPSカメラ (WASD+マウス)、Orbitカメラ (回転/ズーム/パン) |
| resource | 309 | 12 | 非同期リソース管理、参照カウント |
| bridge | 642 | 12 | ALICE-xxx連携トレイト (`SdfEvaluator`/`CollisionProvider`/`AudioSampleProvider`/`WorldProvider`/`TextProcessor`/`AnimationProvider`/`NetworkTransport`/`SdfFontProvider` 等)、プラグインシステム |
| easy | 295 | 9 | GameBuilder + Game 高レベルAPI (5行ゲームセットアップ) |
| query | 293 | 11 | 型安全ECSクエリ (query2/3)、フィルター、SystemScheduler |
| gpu_mesh | 280 | 9 | GpuMeshDesc、VertexLayout、DrawCommand/DrawQueue |
| simd_eval | 268 | 8 | SIMD 8-wide SDF評価 (wide f32x8)、Vec3x8、バッチeval |
| lod | 264 | 13 | LODグループ選択、スクリーンカバレッジ、バッチカリング |
| window | 263 | 15 | WindowConfig、キーマッピング、FrameTimer |
| **合計** | **20,840** | **744** | |

## 機能フラグ

| フラグ | 説明 |
|--------|------|
| `gpu` | wgpu ディファードレンダラー (Vulkan/Metal/DX12/WebGPU) |
| `window` | winit ウィンドウ + GPU (`gpu` を含む) |
| `sdf` | SDF評価、Marching Cubes、球トレース |
| `audio` | HRTF空間オーディオ、バスルーティング、エフェクト |
| `ui` | 保持モードUIウィジェットシステム |
| `particles` | パーティクルエミッター |
| `navmesh` | ナビメッシュ + A* + 群衆 |
| `ffi` | C/C++/C# FFIバインディング |
| `python` | Python (PyO3) バインディング |
| `godot` | Godot GDExtensionバインディング |
| `full` | 全ランタイム機能 (ffi/python/godot を除く) |

## 品質

```bash
cargo test --features full        # 843 テスト
cargo fmt -- --check              # 0 差分
```

## ALICE Eco-System 連携

`bridge` モジュールで以下のトレイトを定義:

| トレイト | 接続先 (実例) |
|---------|---------------|
| `SdfEvaluator` | ALICE-SDF (`CompiledSdf`) |
| `CollisionProvider` | ALICE-Physics |
| `AudioSampleProvider` | ALICE-Audio デコーダー |
| `MeshProvider` | ALICE-SDF Marching Cubes 出力 |
| `ShaderTranspiler` | ALICE-SDF HLSL/GLSL トランスパイラ |
| `WorldProvider` | テーマ別ワールドバックエンド (ZoneId/Weather/Teleport) |
| `TextProcessor` | **ALICE-Text** (`AliceTextProcessor` adapter、自然言語ログ圧縮) |
| `AnimationProvider` | ALICE-Animation |
| `SkeletonProvider` | スケルタルアニメ provider |
| `SdfFontProvider` | ALICE-Font |
| `NetworkTransport` | ALICE-Sync 等のトランスポート |
| `StreamingProtocol` | ALICE-Streaming-Protocol |
| `UiRenderer` | カスタムUIレンダラー |
| `Plugin` | 任意の拡張プラグイン |

trait は engine 側に定義のみ、adapter 実装は下流クレートに置く設計
(`alice-game-engine` が ALICE-Text 等に直接依存しないため、各ゲームが
必要なバックエンドだけ選択可能)。

## ライセンス

**MIT** と **Commercial** のデュアルライセンス。

- **MIT** — オープンソースおよび年間$100K未満の商用利用はアトリビューション付きで無料。[LICENSE](LICENSE) 参照。
- **Commercial** — プロプライエタリSaaSまたは高収益製品には商用ライセンスが必要。[LICENSE-COMMERCIAL](LICENSE-COMMERCIAL) 参照。

お問い合わせ: sakamoro@alicelaw.net
