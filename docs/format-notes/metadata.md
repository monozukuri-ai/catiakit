# 名前付きストリーム内のメタデータ

メタデータ検査はコンテナ検査で受理した外側ディレクトリの`CATPropertySets`、`CATStorageProperty`、`CATDocumentProperty`、`CATPreview`だけを対象にする。物理extentを再構成してから論理位置で読み、取得範囲を元の物理範囲へ対応付ける。コンテナ・ディレクトリの余白、`Data`、内側ストリーム、任意のファイル内文字列からメタデータを採用しない。

保存値のタグと参照atomの参考資料は、cadmpeg projectの[CATIA形式資料 §3.3](https://github.com/cadmpeg/cadmpeg/blob/6289a0d31901aae9445fb99c4bbf7c8effd1d3f1/docs/formats/catia.md#33-finjpl-segments)、コミット`6289a0d31901aae9445fb99c4bbf7c8effd1d3f1`。[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/)による資料を参考に独立実装した。FINJPLのマーカー探索ではなく、NIST入力の名前付きストリームを根拠に、以下の境界・順序の制約と未知領域の保持を追加した。2026年11月3日の拡張では、NISTの20件に加えてローカルの第三者サンプル65件（V5R12〜R21、非配布）で文法を照合した。

## プロパティセット

`CATPropertySets`の先頭は`01 01 00`、u32 LEのset数。各setはu16 LE長＋ASCIIのset名で始まる。set名の直後にu32 LEのproperty数と、その数だけのpropertyが続くのがinline形。`CATPreview`はset名だけで本体を持たず、次のset名またはストリーム末尾が続く。先読みで両形を区別する。u32がproperty数の上限内で、0か、直後にu16長付きの印字可能ASCII名が続けばinline形。そうでなく、その位置がu16長付きの印字可能ASCII名ならname-only形。どちらにも合わなければ無効、上限を超える数値は資源制限として拒否する。

| 項目 | 表現 |
| --- | --- |
| property名 | u16 LE byte長＋ASCII |
| 値の型 | u32 LE |
| 型2 | u16 LE |
| 型3 | i32 LE |
| 型12 | u32 LEの要素数。各要素はu32 LEの型＋値。入れ子のlistは未対応 |
| 型13 | u32 LE byte長＋bytes。hexで保持 |
| 型14 | u32 LE byte長＋文字列。印字可能ASCIIならtext、それ以外はhexで保持 |

観測したsetは`CATSummaryInformation`（inline）、`CATPreview`（name-only）、`CATContainerLinksUpdateInformation`（inline）、`CATProjectFlags`（inline、型12の中に型13）。未知の型で再同期せず、そのscopeを未対応として残す。

`CATSummaryInformation`内の`LastSaveVersion`（型14）だけを保存バージョンとして採用する。値は`Version`、`Release`、`ServicePack`、`BuildDate`、`HotFix`の順の`<名前>値/<名前>`を完全に消費する形。V5R12の値は`BuildDate`で終わり`HotFix`を持たないため、`hot_fix`は省略可能とする。BuildDate以外は非空のASCII数字列、BuildDateは非空の印字可能ASCIIで、`/`・`<`を含まない値に限定する。各値は64 bytesまで。年式や製品名への変換はしない。

同一のtupleが複数あれば観測を全て残し、代表値を一つ返す。異なるtupleなら`conflicting`で代表値なし。破損・未対応のsummary候補があれば、正常な別観測があっても代表値を確定しない。`FirstStreamed`はLastSaveVersionの代用にしない。`MinimalVersionToRead`、`CATBuildLevel`、`CATPDMPersistence`、`OMBDocument_0001`等の他のpropertyは型付きの値として`property_sets`に保持し、意味は割り当てない。

## ストレージ宣言

観測した`CATStorageProperty`は先頭byteが`M`（V5R21以降）または`N`（V5R14〜R19）で、論理位置126にroot atomがある。126 bytesのpreambleは意味を固定せずopaqueとして保持し、先頭byteは`envelope_byte`として記録する。現在はこの配置に限定し、root atomを範囲内から検索し直さない。

rootは`34 12 CATStorageProperty`、`80 01 00 00 00 00 22 0c 00 00 00 34 01`、kind byte、`00`の順。続いてクラス宣言`34 10 CATUnicodeString`、文字列instanceが二つ（各`a0 02 00 00 00 00`、文字列atom、`9f`）、16 byteのID（`3b 10`＋16 bytes）が一つ以上、`9f`、末尾`36 08 "FINJPL  "`の順。文字列atomは`34`＋u8長＋bytesが通常形で、`CATMFBRP`だけは長さbyteを持たない`20`＋8 bytesの形で保存されている。後者は観測形として受理し、一般化しない。末尾より後のbytesはopaqueとして残す。

| kind | 役割 | 第一文字列 | 第二文字列 | ID |
| --- | --- | --- | --- | --- |
| 1 | 保存時の文書名 | `CATIA` | 保存名。V5R21以降の全例で空、V5R14〜R19では保存時のファイル名 | 16 zero bytes |
| 2 | 文書種別 | `CATPart`、`CATProduct`、`CATDrawing`等 | 同じ型名 | zero＋文書ID |
| 3 | application container | `CATProdCont`、`CATPrtCont`、`CGMGeom`等 | container名。`CATCameraContainer`/`Camera`のように型と異なる例がある | container ID |

kind 2の型名で`DocumentInfo.kind`を決める。`CATPart`、`CATProduct`、`CATDrawing`、`CATShape`以外の型名は`declared_type`に文字列のまま残し、kindは`unknown`。kind 2またはkind 1の宣言が複数あり内容が異なれば`conflicting`で代表値なし。kind 1の名前は文書自身の名前であり、外部文書参照ではない。V5R14のCameraStartupContainerが使う`22 01 00 00 00 22 03 ...`で始まるkind 0の形は未対応scopeとして全体をopaqueに残す。文字列はUTF-8として解釈し、解釈できないものは未対応とする。

## 文書リンク表

`CATDocumentProperty`は同じenvelopeとpreambleを持ち、論理位置126に`34 13 CATDocumentProperty 80 01 00 00 00 00`、`22`＋i32、`22`＋i32（宣言数）が続く。以後はentryの列で、最初のentryの前にクラス宣言`00 34 11 CATSymbolProperty`、最初のoctet arrayの前に`00 34 0d CATOctetArray`が一度だけ現れる。

各entryは`e0 02 00 00 00 00`、`22`＋i32、`e0 03 00 00 00 00`、`22`＋i32のoctet数、octet数に達するまでの`3b`＋u8長＋bytes、`9f`、`22`または`23`＋i32が3個、`21`＋i16が2個、`9f`。3個目のi32はV5R14〜R17で`23`のタグを持つ。entryの列の後に`24`＋u32（V5R12では`22`＋u32）の表が続き、`9f`と`36 08 "FINJPL  "`で終わる。宣言数はentry数と一致せず、意味は割り当てない。数値とタグは`values`、`value_tags`、`shorts`、`index_table`、`index_tag`としてそのまま保持する。

octet payloadはlocatorで、u8長＋識別子、u32、u8長＋種別名、u32の残長、`05 DocId`、u16の残長、`01`、`04 File`、u8長＋文書型、任意でu16長＋pathの順。観測した種別名は`GenericLocate`（識別子`6f`）、`CATIA`（識別子は`03`＋16 byte UUID）、`CATWin`（識別子`Undef`）。残長はそれぞれ以後のbyte数と一致しなければならない。文書型は`CATPart`、`CATProduct`、`CATDrawing`、`CATMaterial`、`CATUnloadable`、`feat`を観測した。pathはUTF-8として解釈し、解釈できなければhexで保持する。文法に合わないpayloadはhexで保持し、`metadata.link_invalid`を警告する。

最初の`GenericLocate`entryは文書自身の保存時のpathを持つ。`feat`は使用したfeature catalog、CATProductの`CATIA`entryは構成部品の保存path、CATPartの`CATIA`entryは関係する他文書を指す。これらは保存文字列であり、参照先を開く処理はない。

## プレビュー

`CATPreview`では先頭4 bytesを未解明ヘッダーとしてhexで保持し、その直後からJPEGを読む。ヘッダーの値を0に固定して検証しない。JPEGの構造は[ITU-T T.81 Annex B](https://www.w3.org/Graphics/JPEG/itu-t81.pdf)を根拠とする。

SOIから、長さ付きsegment、SOF/SOS、走査中のbyte stuffing・restart markerを辿り、EOIを境界として返す。APP/COM内のEOI風バイトはsegmentの長さ内に収まり、終端にならない。SOF0/SOF2、8-bit sample、1〜4成分を対象に、長さ、成分ID、寸法、走査ヘッダー、上限を検証する。その他のcoding processは未対応。DNLによる高さの後置定義は扱わない。

これはmarker framingの検査であり、係数の復号、Huffmanコード表の完全な妥当性、progressive scan間の完全性、MCU数、画像の意図した見た目の保証ではない。画素バッファを生成せず、JPEG byte数・画素数・走査仕事量を制限する。

STC 5件には最初のEOIの後にもbytesがある。画像範囲は検証したEOIまでとし、その後をopaque rangeと`preview.trailing_data`警告で保持する。追加画像や意味を推定しない。他の15件はEOIがstream末尾に一致する。調査時に保存した末尾までのpayloadを、画像範囲と同じものとして扱わない。

複数の受理したプレビューは別IDで保持する。ID省略時は、候補が一つで、破損・未対応のpreview scopeがない場合に抽出する。明示IDでは受理済みの候補を選べる。抽出は検査と同じ入力バイト列を使い、headerやopaque tailを出力画像へ含めない。

## 状態と出典

`saved_by`、`storage`、`links`、`previews`はそれぞれ`not_found`、`present`、`invalid`、`unsupported`等の状態を持つ。保存情報や宣言の矛盾は`conflicting`、リソース制限等で検査を完了できなければ`incomplete`となる。`incomplete`の検査では文書種別を確定しない。scope一覧、観測値、診断を合わせて確認する。`not_found`はこの実装が検査した文法上での未検出であり、文書全体の不存在を証明しない。

`StreamSpan`は数値stream ID、論理開始・長さ、論理順の物理範囲列を持つ。断片化した値を包含する単一物理範囲に置き換えない。未知の内容はscopeのopaque rangeとして残し、元の入力と対応付けて回収できる。JSONには画像本体を入れない。

## 未対応のまま残す項目

`CATProdCont`の本体（OSMX/SPEC形式）には`_PartNumber`、`_Nomenclature`、`_Revision`、`_DescriptionRef`の属性名と、Part Numberらしき文字列が隣接して保存されている。属性と値を結ぶ参照文法を解読していないため、文字列の隣接だけでPart Number等を割り当てない。ディレクトリ内の`@Cont_`、`6...`で始まるstream名は、UTF-16の長さ語を名前の先頭文字として読んだものである。名前の修正はコンテナ照合の記録と合わせて別途行う。
