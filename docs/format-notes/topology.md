# 標準indexed topologyの受理条件

トポロジー復元は幾何復号の一意なpopulation、counted vertex、surface/support rosterを前提に、indexed tableとtrim packetを照合する。対象はline/circleと明示的collapsed pointで境界を表せるanalytic carrier。plane/cylinderに加え、cone/sphere/ring torusの導出chartを扱う。body/region/shellは接続から導出したグループで、native body IDやmaterial regionの包含関係を解読したものではない。

追加の解析intersectionは、serialized incident carriersが選ぶellipse、sphere-plane circle、cylinder-cylinderの一つのquadratic-root branchである。候補は全occurrenceのsampleで一意に選ぶ。通常の角度区間には3点以上の異なる座標と単調なangle列を要求する。2点の円弧は、同じcircleの他のnative端点を含まない一意な弧、または対応する円筒・トーラスの保存face-side witnessで一意に選べる場合だけ採用する。区間の評価には実数domain検査があり、native parameterizationそのものを解読したとは扱わない。

参考資料はcadmpeg projectの[CATIA仕様](https://github.com/cadmpeg/cadmpeg/blob/6289a0d31901aae9445fb99c4bbf7c8effd1d3f1/docs/formats/catia.md)、commit `6289a0d31901aae9445fb99c4bbf7c8effd1d3f1`。[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/)の資料を参考に独立実装した。本実装は以下の厳密なendpoint照合、mesh境界との完全対応、derived chartでの向き、未解決bodyの保留を追加する。参考実装を実行時には使用しない。

## Handleと頂点の結び付け

選定したMainDataStreamのtrim chainより前だけを走査する。`ff ff 02 00 01 ff`に続くu32 LEのentry数、`ff`、補助u32、3 zero bytes、coding modeを読む。mode 1はentry数×3のf32 LE。mode 0はceil(entry数/4)のcontrol bytes、`ff`、scalar数u32 LE、f32 LE scalar列。2-bit controlをLSBから取り出し、0=XYZ、1=前値の全成分再利用、2=XY再利用・Z読み取り、3=X再利用・YZ読み取りとする。最初は0、scalar消費数は宣言と完全一致、座標は有限値である必要がある。

補助u32は意味未確定で保持する。参考資料の二つのcountが近いという記述は受理条件にしない。CTC03はentry数2,934、補助値2,870で、差は64である。

全edge/trim handleがtable範囲内であることを確認する。edge行の最初と最後のhandleが示す座標を、counted native vertexのbinary32表現にbit単位で照合する。全native vertexを覆う候補表が一つだけ必要。同じ位置のnative vertexが複数なら、座標を統合せず未解決にする。最寄り点、丸めた座標、推測したnative IDを使わない。index tableは端点の結び付けと幾何整合性の確認に使い、曲面や曲線のcarrier定義を置き換えない。

## Trimからの境界復元

packetのA独立三角形、B strip、C fanを展開する。stripは交互にwindingを反転する。同じhandleを繰り返す縮退三角形は境界を供給しない。異なるhandleは座標が等しくても統合しない。三角形の内部共有辺は逆方向で2使用、境界は1使用を要求し、3使用以上や同方向の内部共有を拒否する。残った境界は各handleで入出次数1の閉じたcycleに限る。

cycle上のnative endpoint位置を候補として、全境界segmentを一度ずつ覆うphysical-edge配置を探索する。fullの複数edge tableでは両端portを除いた列、compact単一tableでは全列を、順方向または逆方向で照合する。handleが異なっても、indexed座標のbit単位で一致するaliasを対応付けられる。三角形incidenceではaliasを統合しない。fullでarity 2のlineは、対応する全sampleの共線性とnative endpoint区間も必要。native pointと一致する中間sampleがあることだけでは辺を分割しない。全配置が一意で、全edge incidenceが満たされる必要がある。

異なる二つのserialized face slotは両ownerを制約する。repeated slotは一つの既知ownerを制約し、完全indexed occurrenceによってもう一つのownerを復元できる。raw slotは変更しない。1使用または2使用の条件、known ownerの包含、全cycleの閉鎖を確認し、repeated slotだけでseamと断定しない。

未解決planeの位置はtrim normalとcounted native pointから決め、incident pointすべての平面整合性を確認する。明示的line supportだけをcounted native endpointsから配置し、全trim sampleが区間上にあることを確認する。boundsは引き続き定義に使用しない。circleの区間は全trim sampleからangleを連続化して求め、incident occurrence間で一致を要求する。短い方の弧を推測せず、反対点だけでは弧を確定しない。

## UV、seam、向き

planeは直交chart、cylinderは角度と軸方向、coneは角度と母線距離、sphereは経度と緯度、ring torusは二つの角度のchartを導出する。各loopで周期座標を連続化する。periodic方向に閉じないcycleは、明示的なseamまたは対応するpolar runがなければ保留する。同じedgeの同じfaceでの2使用はseamとして保持し、coedgeごとに異なる2π branchのpcurveを持つ。

単純pcurveはplane上のline/circleとcylinderのgenerator/section circle。それ以外の対応analytic carrierでは、3D解析曲線の評価点を解析的に逆写像する`analytic_projection`を保持する。phase guideは周期branchだけを選択し、幾何を補間しない。coedgeの正規化parameter 0〜1を方向付きedge intervalへ対応させる。曲面へのliftと3D edgeをサンプル照合し、UVでの連結とloop閉鎖も確認する。UV polygonの自己交差、loop同士の交差・接触、複数outer、innerの入れ子を拒否する。これはbounded sample検証であり、全曲線間交差やsolidの妥当性の証明ではない。

`native_sense_byte`は記録されたsignを保持する。`chart_sense`は導出したchartでouter/innerとradial pairの向きが整合するように求める。nativeのUV軸が未確定なため、raw signを導出chartに二重適用しない。CTC03ではplane 23面でこの二つのsignが異なる。loop全体の反転の制約を解き、共有辺の2 coedgeは逆方向、outerとinnerは逆windingにする。全体を反転できる成分の向きは決定的なgaugeとして記録し、material outwardの確定とは区別する。

## 所有関係と未対応

body→region→shell→face→loop→coedge、edge→vertex/curve、coedge→pcurve/surfaceをIDで結ぶ。1使用edgeはopen、2使用はradial pair。vertex linkが一つのcycleまたはpathとなることを確認し、bow-tieや分岐を拒否する。shellの連結性、閉鎖とEuler characteristic `V-E+Σ(2-loop数)`を検証する。`validate_topology`は公開・JSON復元したIRでも参照、所有、UV chain、liftを再検証するが、出典を認証する機能ではない。

`closed_shell`と`open_shell`を区別し、`solid_validated`は常にfalse。open shellの作者意図は未解読で、`expected_closure`は呼び出し側の条件である。条件不一致はグラフを残しつつexportを保留する。`export_eligible`は後続の外部検証アダプタへ渡す候補を表し、製造可能性を表さない。

明示的line supportの端点が同じnative vertexで、異なるhandleが円錐apexまたは球poleの連続runを示す場合だけ、`Point` locusと非零のUV区間を保持する。一つのpolar run、極上の同一点、対応するcollapsed occurrence、前後の非特異境界が必要。vertex linkはcollapsed useを越えて接続し、Euler計算はcollapsed edgeを1-cellから除く。他のゼロ長occurrenceは`topology.degenerate_uv_unresolved`で保留する。このproductionの正例は構築fixtureであり、native保存例は未認定。

freeformと一般的な交線support、複数polar run、horn/spindle torus、material nestingは未対応。解析UVが未完了でも、完全に復元した端点・ordered cycle・incidenceは`boundaries`へ残し、`validate_boundaries`で再検証できる。これはexportable bodyではない。失敗したpopulationの幾何変更・完成topologyは確定しない。bytes/records/workの制限停止はrejected。既に確定した別populationの証拠は保持する。

一つのnative cone apexに母線が集まる場合、UVのincoming/outgoing incidenceを分け、constant-point connectorをloopの`singular_transitions`へ保持できる。native vertex recordと両UV incidenceを照合し、connectorの全sampleが同じ頂点へliftすることを要求する。nativeにないphysical edgeは追加しない。Native CTC01で検証した。これは明示的collapsed line productionやnative UV payloadの解読とは別である。

未完成bodyの独立した解析useは`analytic_partials`に保持する。refined surface/curve、native boundary occurrence、使用方向、interval、pcurveと未解決useを分ける。閉じたedgeも方向を明示し、端点が同じことから向きを推測しない。`validate_analytic_partial`で参照・端点・3D/UV embeddingを再検証する。この部分結果はloop分類・shell完了を保証しない。

`native_uv`は別の保存record層である。leaf streamの長さ付きA/B chainからclass-20のUV quintic jet、対応するB-family analytic carrier、A-family class-34のclamped NURBSを直接復号する。原frameとsource位置を保持し、`validate_native_uv`がfieldとraw bytesを照合する。native parameterでの評価とnormalized coedge parameterでの評価を区別する。header operand・persistent support tag・allocation referenceの数値一致だけではcurrent faceへ結び付けない。現在のcoedgeへのbindingを伴わないhistorical recordも保持する。

## 保存carrierを使う限定binding

保存full-sphere class `2a`と隣接する3本のclass `18` parameter lineが、閉じたframe chainで角度・子午線関係を満たす場合、`native_uv.sphere_meridians`へnormalと原frameを保持する。同じsphere locusを持つ両owner、binary32のcenter/radius再現、反対極の端点、両側の完全trim pathを照合してcircle planeを確定する。平面の最小二乗fitは行わない。重複radiusはbit一致を要求し、演算で得るrange-originだけ浮動小数点の丸め上界を使う。

球面には`SphereStereographic` chartを導出できる。frame座標で`P(u,v)=R(2u,2v,1-u²-v²)/(1+u²+v²)`、除外点は-Z pole。packet全体が候補pole側の閉半球に収まり、明示的Point edgeがない場合だけ使用する。sampleのmeanと解決済みcircle normalはchart gaugeの候補であり、3D sphere定義を変更しない。

未知freeform faceと保存clamped NURBSの4 cornerがそれぞれ一つのcurrent native vertexに対応し、全physical boundary occurrenceが一意なnative rectangle side上にあり、4辺を隙間・重複なく覆う場合だけbindingする。辺の分割は許容する。全pathと端点は従来の1e-3 mm gate、向き付きparameterは単調性を要求する。内部が異なる複数carrierは、境界が同じでも採用しない。導出元は`derived_unique_native_nurbs_rectangle_incidence`で示し、persistent native identityの解読とは区別する。

NURBS曲線は正の重みとclamped knotで評価する。境界の逆写像は評価回数を制限した数値探索で、元の有理曲線の3D membershipを再検査する。control net・knot・weightの複製と数値仕事量をrecords/work上限へ算入する。部分UVも採用前にnative端点を確認する。

単一loop・3 native vertexのtrimは、矩形を覆うという条件を課さず、全端点・全境界path・内部packet witnessが一致する保存NURBSを一意に限定する。異なる候補が複数あれば保留する。`derived_unique_native_nurbs_triangular_trim_correspondence`はこの導出関係を表す。既知の矩形sideは従来の境界逆写像を使い、それ以外は正確な有理偏微分を使うbounded full-domain inverseを追加課金して試みる。所属witnessを得られない場合やJacobianが特異な場合は採用しない。失敗は非所属証明ではない。

## Rolling-ballとnative surface-jet境界

A-family class `32`はK knotと10-channelのvalue/first/second lanesを持つ。Limit1、Limit2、Center、angleの全quintic jetを保持し、`RollingBallJet`はそのまま回転曲面を評価する。tailのreference scalarはball radiusではない。sourceとfieldは`validate_native_uv`で再検査する。

全native vertex・境界pathと内部packet witnessで一意なcarrierを限定し、現在のfaceへの導出対応を記録する。limiting channelはexact quintic curveとして保持する。native analytic carrierにUV jetをliftした`SurfaceJet`、およびqualified analytic carrierとrolling-ballのscalar-root交線も全端点・path・liftを検証する。未知のpersistent support identityを確定したとは扱わない。

一定U断面では、第一native端点の逆写像からstationを得る。保存jetのCenter、Limit1/2からcircleを定義し、第二端点、全path、隣接carrierへの17 sample liftを検査する。stationを平均したり、circleをmeshへfitしない。複数rolling carrierから得る候補も一致を要求する。出典は`derived_native_rolling_ball_constant_station_section`で示す。

NURBS tailの`current_limits`はspan-local scalarで、`limit_span_indices`と区別する。`knot_limits`は解釈できる場合だけ、元knot座標へ戻した限界を保持する。affine parameter mapは別の保存データである。

## Scaled-reference ruled jetとnative chart

A-family class `39`の限定grammarはdegree 5、K knot、6-channelのP/Q value・first・second、48-byte tailを持つ。current U limitsは保存knot上、reference Vは`[0,d]`、current Vは`[0,2d]`で`d>0`を要求する。保存siteで`|Q-P|≈d`を検査し、`S(u,v)=P(u)+(v/d)(Q(u)-P(u))`を評価する。site間でrulingを正規化しない。Hermiteから得るPと`2Q-P`をcontrol列とするdegree `[5,1]`、weight 1のtensor NURBSへ正確に変換する。一般のclass `39`を認定するgrammarではない。

単一loopの全native vertex・全境界path・最大8個の内部witnessで一意な面だけをbindingする。出典は`derived_unique_native_scaled_reference_ruled_jet`。保存V limit curveも全occurrenceの端点・path・stationの単調性を確認して`derived_native_ruled_limit_with_complete_path`で記録する。native persistent support IDは未解読である。

degree V=1、V control数2、同一weight、各control列で一つの座標がbit一定という保存構造では、Vを直接求めてUだけをbounded curve inverseへ渡す。3D残差と非特異Jacobianを再検査する。その他は既存のfull-domain inverseを使い、採用する経路に応じたworkを課金する。任意の高コストbindingは、後続pcurve構築・再検証用の予算も予約してから実行する。

compressed cylinderの106/114-byte frameでは、control byteをX/Y basisの6成分に対するzero-elision maskとして解釈する。長さ`122-8*popcount(mask)`、直交単位frame、正radius、冗長range/shift値を検査する。coneの184-byte frameはreference origin O、radial-plane angle a、reference radius r、独立angular scaleを保持する。native slant Lで`P=O+(r+L cos(a))*R(theta)+L sin(a)*A`。neutral coneのapexは`O-r tan(a)*A`、slant shiftは`r/cos(a)`であり、r=0でもangular scaleを保持する。

`SurfaceJet`は保存UV jetを変更せず、`S(jet(t)*uv_scale+uv_offset)`で評価する。JSONの`uv_offset`省略時はzeroを既定とする。native analytic intersectionを先に解決し、疎なpathのellipse等も、保存jetを既知analytic locusと全ownerで検査できる場合だけ復元する。membership・端点のcacheはworkを課金し、全path検査を省略しない。

rolling-ballのsource U端でangleが厳密にzero、両contact limitが一致し、native counted vertexへ一致する場合、V方向の導出UV connectorを許す。全9 lift sampleを既存gateで検査し、`derived_chart_connector_at_counted_native_rolling_tip`を記録する。物理辺は追加しない。保存limit curveに厳密一致する曲線は、tipでも元のsideのUVを維持する。

## 個別NURBS limitと完全な円筒frame

class `28`のgeneral cylinderはorigin XYZ、6-bit zero-elision mask、X/Yの非省略成分、radius、4 bounds、terminalを持つ。terminal `03`はshiftを追加し、payload長は`122-8*popcount(mask)`。terminal `07`はshiftを持たず、長さは8 byte短く、U幅が`2*pi*radius`に一致する場合だけ認める。mask=0の完全frame、4成分を省略する90-byte form、full-periodの82-byte formも同じ規則で読む。直交単位frame・正radius・有限値・完全消費・bounds・冗長shiftを検査する。別の既存圧縮grammarは、このgeneral grammarを通らない場合だけ試す。

`nurbs_limits`は、面全体のbindingと独立して保存NURBSの4 limit curveを調べる。一定の正weightと単調control座標を要求し、各degree+1のcontrol列で厳密な進行があることを検査する。少なくとも一つのcurrent ownerが解決済みであり、両native端点・全occurrence path・向き付きparameterの単調性・17 stationの全既知ownerへのliftが従来gateを満たす場合だけcurve候補を保持する。異なる候補が残れば保留する。出典は`derived_native_nurbs_limit_with_complete_path_and_known_owner`。面全体やpersistent native support IDの対応を確定したとは扱わない。境界点だけが一致しても、曲線途中が既知ownerを外れる候補は採用しない。

疎なellipseにもco-circularと同じ補弧排除を適用する。同じnative incidenceを持ち、同じcenter・両radius・major axis・planeを持つ別のellipse edgeのnative vertexが、候補の片方の弧内部へ入る場合、その弧を除外する。別のlocus、曖昧な両弧、carrier外のvertexからは区間を作らない。両ownerのpathとnative端点の再検証を維持する。

全curveを解決できても、未知面が残る場合はcomplete UVへ進まず、公開validatorで再検証できる部分pcurveと元のboundary graphを保持する。

## 保存parameter relationとNURBS周辺UV program

class `24/25`のscalar relationは`82`、二つのallocation operand、`0a/0b`とselectorで始まる。target rangeとsigned accuracy、selectorが選ぶsource range・affine係数または対応標本列、target site、必要なfit wordまで完全消費する。A-familyの長いclass `25`も同じ限定payload grammarで読む。width-coded、backward-distance、owned-child、selector-2、明示u8/u16のoperandを同一視せず、raw bytesも保持する。参照namespaceのcurrent bodyへの解決は未実装である。

全scalar・range幅は有限、rangeは昇順、affine係数は非zeroで端点式が一致することを要求する。標本数は2–127、target列は昇順、source列は昇順か降順のどちらか。標本は各source/target domain内にある。signed accuracyの符号付きzero、別のfit word、selectorが表すsiteの有無を保持する。`image_at`はaffine/identity、または保存した標本対だけを評価し、sampled relationの内挿は行わない。

保存A-family NURBSの直後に4個のB-family class `18`が連続する場合、対応するsource context全体を保持する。既知のconstant-U/V、明示direction、implicit U/V-axis programだけをtyped fieldとして読む。implicit-axisのsenseは未割当てのまま、両方のUV候補を返す。未対応controlもcontextのraw bytesへ残す。隣接関係はpersistent support identityやcurrent face/coedgeへのbindingを与えない。

公開validatorは元frameとのfield一致、signed-zero bits、source scope、5-frame context、NURBS sourceとline ordinalを再検査する。保存fit wordを3Dの全域error boundとして扱わない。

class `18` control `00/40/80`は対称性による上端省略を行わず、`t0,t1,originU,originV`を明示的に保存する。`00`には単位directionの二成分が続き、`40/80`はそれぞれV/U軸のlocusでsenseは未確定である。`08/48/88`は従来どおり負の`t0`から`[t0,-t0]`を読む。有限な幅・両端UV・完全消費を検査する。旧JSONがraw context内に新対応の3 controlを未解読のまま保持することは認めるが、保存済みlineのfieldとordinalを再検査する。

`native_uv.parameter_map_contexts`は、既知のparameter mapに同一runで直後のB-family class `06`二frameを関連付ける。各payloadの`80+n`に続く`n`個のallocation referenceと非空tailを保持し、tailはopaqueである。三frameのsourceが完全に隣接すること、同じrun/streamに含まれること、元frameとすべてのfieldが一致することを検査する。値が`(a,b)`から`(1,a+1,b+1)`、`(2,a+2,b+2)`へ進む例は算術的な観測であり、端点・面・辺のidentityを与えない。
