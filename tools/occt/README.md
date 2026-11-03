# Open CASCADEによる外部検証

catiakitが復元したB-repをOpen CASCADE Technology（OCCT）のPythonバインディングOCPで再構築し、kernelの妥当性検査と、NISTが公開するSTEP参照との比較を行う。開発用の検査ツールであり、パーサ本体にもRustコアの実行時依存にも含まれない。パーサの出力は変更せず、復元結果をパーサへ書き戻すこともしない。`solid_validated`はパーサ側で常にfalseのままである。

## 必要な環境

- Python 3.11以降と`cadquery-ocp`（OCCT 7.8系で確認）。`python3 -m pip install cadquery-ocp`
- releaseビルドのCLIと`sample_geometry` example。`cargo build --release --workspace --bins --examples --locked`
- 取得済みのNISTコーパス。`python3 tools/corpus/corpus.py fetch`

## 実行

```sh
python3 tools/occt/compare_step.py
python3 tools/occt/compare_step.py --case ctc-01 --case stc-06 --output path/to/report.json --step-dir path/to/step
```

`compare_step.py`は`tests/corpus-manifest.toml`の開発分割にあるCATPartごとに、CLIの`decode --topology --json`を実行し、`sample_geometry`で手続き的なcarrierを標本化し、`catiakit_occt.build_shape`で形状を構築して、同じcaseのSTEP候補すべてと比較する。評価用の分割は読まない。`--records`と`--steps`はtopology予算、`--samples-per-edge`と`--grid`は標本密度、`--cli`と`--sampler`は実行ファイルの位置を変更する。`--step-dir`を渡すと再構築した本体をSTEPとして書き出す。報告は既定でGit管理外のローカルディレクトリに書き、`--output`で変更できる。

`sample_geometry`は`decode --topology --json`の出力を標準入力から受け取り、各edgeの曲線をそのparameter区間で、各pcurveを正規化parameterで、rolling-ball曲面を格子で評価したJSONを書く。引数は`[samples_per_edge] [grid_u] [grid_v]`（既定33、25、13）。

## 構築の規則

- 平面、円柱、円錐、球、トーラス、NURBSはcatiakitと同じparameter付けの厳密なkernel primitiveで作る。円錐のapexはv=0、球とトーラスは復元したframeを使う。
- 多arcのNURBS（内部knotの多重度が次数+1）は、接合する極が一致する場合に限り重複する極を落として多重度を次数に下げる。曲線と曲面の両方に適用し、厳密な変換として件数を記録する。極が一致しなければ構築しない。
- rolling-ball曲面は標本格子へのB-spline近似、交線や限界曲線などの手続き的曲線は復元したparameterを与えた補間で作る。近似した面と曲線はモデルごとに記録する。
- pcurveは復元したpcurveをedgeのparameter区間でkernelへ移す。周期的なparameterはkernelが基本周期へ正規化するため、edgeの区間を読み戻して差を吸収し、面ごとにUVを基本周期へ移す。
- 極を通る球面の境界では、回転したchartの候補を試し、必要なら極を頂点とするchartに切り替えて、seamと縮退edgeを合成する。polar connectorは縮退edgeになる。
- 面の向きはouter loopのUV符号付き面積から決める。shellは`BRep_Builder`で組み、閉じたshellはsolidにして`BRepLib::OrientClosedSolid`で向きを揃える。
- 妥当性は`BRepCheck_Analyzer`で検査する。`ShapeFix_Shape`で修復した複製は別に評価し、未修復の結果を置き換えない。

## 比較

体積、表面積、重心、最適bounding box、辺長を整列したbox寸法、boxの中心から重心までの距離、主慣性モーメントを`GProp`の固定次数Gauss積分で求める（適応積分は軸対称な参照で等しい主モーメントを分裂させたため、補助値として記録するだけにする）。参照STEPはsolidだけに絞り、PMI注記の曲線や平面は除く。許容は体積・面積・モーメントが相対1e-4、重心とboxが0.01 mmである。参照が別の配置で保存されている場合に備え、配置に依存しない量での一致と、同じ配置での一致を分けて報告する。

| 状態 | 意味 |
| --- | --- |
| `matched` | 閉じて整合したshellが構築され、同じ配置の参照と許容内で一致 |
| `matched_up_to_placement` | 配置に依存しない量では一致し、配置が異なる |
| `differs` | 構築はされたが、いずれかの量が許容を超える |
| `no_reference` | 比較できるSTEP候補がない |
| `build_failed` | 構築中に例外が起きた |
| `not_built` | topology coverageが`complete`でない |

shellの整合（閉じている、構築に失敗した面がない、自由edgeがない）と、kernelのanalyzerが報告する面単位の妥当性は分けて記録する。analyzerはinner loopを持つ一部の面を`UnorientableShape`として報告するが、個別の交差・分類・向きの検査は通り、solidとshellの検査は`NoError`である。

STEP参照は別のCADシステムの出力であり、差があってもどちらが逸脱しているかは特定できない。面数は変換器の構成によって異なるため、診断情報に留める。評価の前提は報告の`evidence_limits`に記録する。
