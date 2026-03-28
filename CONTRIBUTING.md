# Contributing to ALICE-GameEngine

## Quality Gate

All contributions must pass:

```bash
cargo test --features full
cargo fmt -- --check
```

## Adding a Module

1. `src/your_module.rs` 作成
2. `src/lib.rs` に `pub mod your_module;` 追加
3. Re-exports セクションに公開型を追加
4. テスト作成
5. `README.md` モジュールテーブル更新

## Commit

- Author: `Moroya Sakamoto <sakamoro@alicelaw.net>`
- Prefix: `feat:` / `fix:` / `refactor:` / `test:` / `docs:`

## License

Dual license: MIT OR Commercial.
