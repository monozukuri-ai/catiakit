# 標準幾何レコードの受理条件

幾何の復号は、コンテナ検査で受理した入れ子ディレクトリ内の`MainDataStream`と`SurfacicReps`を対象とする。各名前で論理長が一意に最大のdescriptorを選ぶ。同率最大は未解決とし、先頭のdescriptorを採用しない。`inspect`は幾何を要求せず、`decode_geometry*`またはCLIの`decode`で解析を要求する。

配置と文法の参考資料はcadmpeg projectの[CATIA仕様 §5・§12](https://github.com/cadmpeg/cadmpeg/blob/6289a0d31901aae9445fb99c4bbf7c8effd1d3f1/docs/formats/catia.md#5-standard-nested-v5_cfv2-topology-spine)、コミット`6289a0d31901aae9445fb99c4bbf7c8effd1d3f1`。[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/)による資料を参考に独立実装した。今回の実装は一意な連続walkとcardinalityの照合に範囲を限定し、平面位置の根拠、未知定義・参照の保持、原子的なpopulation確定、リソース制限を追加した。

## Populationの検証

1. 主ストリームで`(30|b0) 04 04 ff`から始まる8-byte行の連続runを列挙する。4-byte payloadはrun内で一致する必要がある。最大runが一意であることを確認する。
2. 直後のcounted edge tableからdelimiterとcounted vertex tableまで順にwalkする。countはu8または`ff`＋u32 LE、handleは選定した幅のBE整数。通常形は幅2、compact形はkind 1の一つのedge tableで幅1〜3。複数の幅でwalkできる場合は未解決。
3. vertex tableの宣言数だけ、`05 08 01`＋3×f32 LEを読む。有限値を要求する。他の位置の同じ署名は点にならない。
4. FBB先頭までちょうど閉じる、面数と同数のtrim packet chainを選ぶ。通常のcount長、幅2のu16 BE長、packed two-stripを区別する。異なる完全chainがあれば未解決。
5. 表現ストリームで、面数と同数のsurface recordに続き、edge行数と同数のsupport recordを持つ連続chainを選ぶ。face参照はそのpopulation内の範囲で検証する。複数の候補は未解決。

候補のmarker列だけではrouteを選ばない。選定後はレコード長で進み、parameterやbounds内のmarker風バイトを次レコードとして読まない。現在は各stream pairの一つの最大populationに限定する。他のpopulationや保存文法はopaqueまたは未解決として残す。

## 曲面と曲線

analytic surfaceはu24 LEのcarrier tag、u16 BEのparameter長、`00 33`、kind、BE f32 parameter、40-byteのbounds、sense byteの順。kindと全長はplane `32/49`、cylinder `33/73`、cone `34/73`、sphere `35/65`、torus `38/77`。senseは`01`または`ff`として使用箇所に保持する。47-byte freeform coreは未知定義として保持する。

cylinder/coneは位置3値、axis X/Y、符号付きradius/half-angleを持つ。Zの半球は符号から復元する。軸のbinary32丸めを考慮して正規化し、許容する単位球丸めを超える値・非有限値・0方向を拒否する。radius、half-angle、sphere radius、torus radiiはそれぞれの正の定義域を検証する。元のparameter bytesはhexでも保持し、不正値をJSONのNaN/null数値へ変換しない。

planeは完全なtrim packetが持つunit frame normalを使う。原点は、隣接cylinderと半径・axis位置が整合するcircleの中心を根拠に決定する。incident circleの中心が同じplaneに整合しない場合や、normalが矛盾する場合は確定しない。boundsはopaqueとして保持し、planeの原点や法線を供給しない。

`60` support rowはu24 LE allocation tag、line/circle/splineのbody、二つのface ordinalを持つ。circleの中心とradiusはBE f32。円平面の法線は、中心・半径が整合する隣接cylinder、または解決済みplaneから取得する。一貫した法線がなければunknown。lineは二つの解決済み非平行planeの交線として定義する。他のline配置とsplineは未解決。

binary32標準レコードとcarrierの位置関係には1e-3 mmの許容差を使用する。新しい入力の成功率に合わせて閾値を変更しない。参照実装がbinary64 carrierで精度を補う場合も、今回の値は元のbinary32を保持する。

## ID・使用箇所・出典

中間表現のIDはpopulationとrecord ordinalから生成し、native tagと別にする。点のallocation IDは、support table以後が点数×7 bytesの完全な`54 <u24 LE> 00 00 00` rosterで、IDが一意かつ昇順の場合だけ位置対応させる。推測したtagの加算や最寄り座標によるID割当は行わない。

同じnative tagに複数の定義がある場合は、全候補と未解決理由を保持する。face/edgeの配列番号、native allocation tag、definition ID、serialized incidence slot、orientation signを区別する。geometry definitionと使用箇所は別の配列。ボディID、edge endpoint、trimmed intervalは今回確定しない。

各定義は論理stream内の範囲と、論理順の物理extent対応を持つ。導出したframeやlineには元のnormal、circle、surface recordの証拠範囲も付く。未知のheader、bounds、trim mesh、未対応レコードはopaque範囲から回収できる。

## 座標・パラメータと制限

標準レコードの座標はworld座標として保持する。mm/radianは選定した文法のconventionとして記録し、native unit宣言の取得とは区別する。populationが確定しなければ単位はunknown。

方向が不足するchartのreference軸は、normalから再現可能な正規直交frameとして導出する。`parameterization`に由来を示す。domainはcarrierの自然な無限区間・角度周期であり、native UVやtrimmed domainを取得したものではない。circleの自然周期が2πでも、edgeのparameter intervalは未解決である。

`Frame3`、`RigidTransform`、曲線・曲面のevaluationは有限値・正規直交性・半径・定義域を検証する。RigidTransformのrotationは基底軸の変換先を列に持つ。反射を剛体回転として受理しない。native位置・方向を変更したfixtureと、IRへの剛体変換の両方で評価点を検証する。

geometry用のbyte総量、record総数、走査仕事量を制限する。候補を検証してからpopulationをまとめて確定し、制限停止で半端なpopulationを追加しない。carrierが全て解決しても、幾何inventory全体や必要なtopologyは未完了なのでcoverageはpartial。`--require geometry`は現時点では成功しない。

## opt-in topologyで追加されるcarrier

`decode --topology`は限定したnative incidenceから`CurveGeometry::Nurbs`、`SurfaceGeometry::Nurbs`と球面の`SphereStereographic` chartを追加できる。通常の`decode_geometry`の対応範囲は変わらない。有理NURBSは格納knot座標で評価し、導出chartとnative保存UVを区別する。[binding条件](topology.md#保存carrierを使う限定binding)を参照。
