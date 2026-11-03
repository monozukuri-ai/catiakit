# コーパスの取得と検査

Python 3.11以降と、任意でPillowを使用する。これらのスクリプトは開発用の検査ツールであり、CATPartパーサ本体ではない。Rustコアの実行時依存にも含まれない。リポジトリのルートで実行する。

## 取得と照合

```sh
python3 tools/corpus/corpus.py fetch
python3 tools/corpus/corpus.py verify
python3 tools/corpus/corpus.py survey
```

`fetch`は`sources.json`に固定した4件のNIST配布ZIPを取得し、サイズとSHA-256を照合してから、CATPart、STEP、PDF、テキストを`data/corpus/`へ展開する。配布元がZIPを更新した場合はハッシュ不一致で停止する。ZIP内のパス逸脱、シンボリックリンク、展開先重複、過大な展開サイズを拒否し、取得済みの正しいZIPは再ダウンロードしない。他のCADソフトのネイティブデータは展開しない。

`verify`は全抽出ファイルのハッシュとメンバー集合を固定ZIPまで遡って照合し、プレビューのハッシュも確認する。`survey`は抽出済みファイルをZIP内のバイト列と照合してから、`tests/corpus-manifest.toml`と`tests/corpus/`の集計を生成する。

| パス | 内容 |
| --- | --- |
| `tools/corpus/sources.json` | 配布元とZIPの固定ハッシュ |
| `tests/corpus-manifest.toml` | 118件のファイル識別情報、出典、比較候補、観測値、開発・評価の分割 |
| `tests/corpus/inventory.json` | 検査が照合に使う固定のバイト列インベントリ（ヘッダー、ストリーム、保存情報、STEPの構造集計） |
| `tests/corpus/archive-inventory.json` | ZIP内の全メンバーと抽出対象の区別 |
| `tests/corpus/summary.json` | コーパス全体の集計 |
| `tests/corpus/topology-expectations.json` | 開発分割16件のtopology復元の期待値（coverage、母集団の件数、body種別、診断コード） |
| `data/corpus/` | ZIP、抽出データ、抽出JPEG。Git管理外 |

インベントリの初期調査には公開の参照実装を使用した。その実行ツールと出力は公開ツリーに含まれない。`survey`は参照出力が手元にない項目を未実行として記録する。NISTの利用条件は[配布ページ](https://www.nist.gov/ctl/smart-connected-systems-division/smart-connected-manufacturing-systems-group/mbe-pmi-0)とZIP内READMEを参照。

## 検査

```sh
cargo build --workspace --bins --examples --locked
python3 tools/corpus/check_m2.py
python3 tools/corpus/check_m3.py
python3 tools/corpus/check_document_properties.py
python3 tools/corpus/check_topology.py
```

`check_m2.py`は入力のSHA-256を確認し、descriptor位置・名前・長さ・extent一覧と、再構成した各ストリームのSHA-256をインベントリと照合する。幾何やメタデータの意味は検証しない。

`check_m3.py`はPillowを用意した環境で実行する。保存バージョンと元位置を照合し、抽出したJPEGのハッシュと寸法を物理範囲からの再読およびPillowの復号結果と比較する。名前変更によるスコープ除外、異なる保存値の追加、保存名ownerの破損を各ファイルのメモリ上で試し、制御した変異ケースを記録する。元のファイルは変更せず、変異データをCATIAが保存したモデルと同列に扱わない。

`check_document_properties.py`は各CATPartについて、文書種別が`part`、全metadata scopeが`present`、`CATSummaryInformation`がinline、`CATPreview`がname-only、リンク表に自身の`.CATPart` locatorがあることを確認する。`--extra 'glob'`で渡したローカルのファイルはハッシュを固定せず集計だけを記録し、合否に含めない。

`check_topology.py`は開発分割のCATPartを既定の予算で`decode --topology`し、各featureのcoverage、復元したグラフと保存UVレコードの件数、bodyの種別、診断コードの集合を`tests/corpus/topology-expectations.json`と照合する。意図した変更の後は`--write`で期待値を再生成する。件数の一致は幾何の正しさを示さない。外部kernelでの再構築とSTEP参照との比較は[tools/occt](../occt/README.md)を参照。評価用の分割は読まない。

結果は既定でGit管理外のローカルディレクトリに書き、`--output`で変更できる。CIの`corpus` jobは同じ検査を実行し、報告をartifactとして保存する。

## 合成fixtureの再生成

`tests/fixtures/`の各fixtureは次のスクリプトで再生成できる。いずれもCATIAが作成した文書ではない。

```sh
python3 tools/corpus/create_synthetic.py
python3 tools/corpus/create_metadata_fixtures.py   # Pillowが必要
python3 tools/corpus/create_geometry_fixtures.py
python3 tools/corpus/create_topology_fixtures.py
python3 tools/corpus/create_native_uv_fixtures.py
```

`create_nurbs_binding_fixtures.py`、`create_allocation_fixtures.py`、`create_orientation_fixtures.py`、`create_limit_fixtures.py`、`create_parameter_line_fixtures.py`、`create_parameter_map_fixtures.py`、`create_reference_context_fixtures.py`、`create_rolling_fixtures.py`、`create_ruled_fixtures.py`は`tests/fixtures/topology/`の追加fixtureを生成する。各fixtureの説明は`tests/fixtures/*/README.md`、文法の根拠は`docs/format-notes/`にある。

## ツールのテスト

```sh
python3 -m unittest discover -s tools/corpus -p 'test_*.py' -v
```

テストは危険なZIP、同サイズの改変、紛らわしいマジック、改訂違い、テッセレーション版の誤選定を扱う。`test_check_topology.py`は期待値との照合規則を扱う。
