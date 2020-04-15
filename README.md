# AtCoder4Rust
RustでAtCoderの問題を解くためのテンプレートを生成します。

## Usage
`atcoder4rust [FLAGS] [OPTIONS] <contest id>`

## Example
全てのオプション等を確認する場合は、`atcoder4rust --help`を実行してください。

### 最も単純な場合
`atcoder4rust abc001`
実行後に`username`と`password`を訊かれ、入力するとカレントディレクトリに`cookie.txt`と`abc001/`が作成されます。ただし、既に`cookie.txt`が存在する場合は何も訊かずに、その`cookie.txt`を用いて実行します。
```
abc001
├── Cargo.toml
└── src
   ├── a.rs
   ├── b.rs
   ├── c.rs
   ├── d.rs
   └── main.rs
└── tests
   ├── a.rs
   ├── b.rs
   ├── c.rs
   ├── d.rs
```

### ログインなしの場合
`atcoder4rust --no-login abc001`
公開されているコンテスト等、ログイン不要の場合は`--no-login`オプションを付けるとログイン無しで実行します。このとき、`cookie.txt`は作られません。

### サンプルケースに通るかの確認
`cargo test`を実行することで各問題に対して全てのサンプルケースに通るかどうかを確認できます。またある問題(例: A問題)に対してサンプルケースに通るかどうかを確認したい場合は、`cargo test --test a`を実行することで確認できます。

## TODO
 * [ ] cookieの有効期限が切れた場合の更新
 * [x] 依存クレートの整理 (特に`tokio`の`features`周り)

## License
 * Apache License, Version 2.0 ([http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0))
 * MIT License ([http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))

## Author
[kbone](https://github.com/kbone)
