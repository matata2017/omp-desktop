#!/usr/bin/env bash
# check-sop.sh — dev-workflow-sop 契约裁判（specified oracle）
#
#   check-sop.sh <sop-file> --capture <baseline-file>   从原文件算基线并落盘 + 查锚点
#   check-sop.sh <sop-file> --against <baseline-file>   比对落盘基线 + 查锚点
#
# 退出码：0 全过；1 有锚点缺失或超硬阈值。
# 锚点分两档：POLARITY 带否定/限定，能抓「留词翻义」；DELETION 只防整条消失，需人工过语义。
set -uo pipefail

FILE="${1:?usage: check-sop.sh <sop-file> --capture|--against <baseline-file>}"
MODE="${2:-}"
BASE="${3:-}"
[ -f "$FILE" ] || { echo "no such file: $FILE"; exit 1; }

# 极性锚点：留词翻义会被抓
POLARITY=(
  '不臆造'
  '批准前不碰产品代码'
  '不加载'
  '不等于交付'
  '模型调用一律先问'
  '不派子代理'
  '不用.*ignore.*压下去'
  '不改代码'
  '不替用户提交'
  '默认目标已含'
  '不能只改文本'
  '用户指令优先'
  '不可简化'
  'UI 品质不缩水'
  '不敲命令'
  '调一次模型'
  '保证一条不漏'
  '直接按代码改'
  '不动手改'
  '只查当前任务 SPEC'
  '范围就是用户说的范围'
  '原文逐行'
  '宁可误报'
  '连续两次内修无效'
  '全量覆盖仅限新建'
  '默认用 edit/追加'
  '报 MISS 先怀疑模式写法'
)
# 删除锚点：只防消失，不防翻义 —— 必须人工过一眼
DELETION=(
  '一次性运维'
  '每次编码响应'
  '可运行 check'
  '隔离前置条件'
  '哈希一致'
  '改规则改那一处'
  '分叉清单'
  'D:/work/huanghao/doc-drift'
  'doc-drift.mjs'
  '--root'
  '本地记账'
  '结论写进任务产物'
  'doctor full'
  '意图冲突'
  '必须/禁止'
  'matata2017/doc-drift'
  '文档漂移 · doc-drift'
  '知识阈值'
  '可证伪'
  '外环'
  '未压缩'
  '对立观点或独立裁判'
  '逐行 diff'
  '改前落盘快照'
)

amiss=0    # 契约锚点缺失
tfail=0    # 硬阈值超限（与锚点缺失分开计，避免混在一个 MISSING 里）
check() {
  local tier="$1"; shift
  for a in "$@"; do
    # `--` 结束选项解析：否则 --root 这类模式会被当成 grep 选项（实测踩过）
    if grep -Eq -- "$a" "$FILE"; then
      printf 'HIT  [%s] %s\n' "$tier" "$a"
    else
      printf 'MISS [%s] %s\n' "$tier" "$a"
      amiss=$((amiss + 1))
    fi
  done
}
check polarity "${POLARITY[@]}"
check deletion "${DELETION[@]}"

SECTIONS=$(grep -c '^## ' "$FILE")
LINES=$(wc -l < "$FILE" | tr -d ' ')

case "$MODE" in
  --capture)
    [ -n "$BASE" ] || { echo "--capture needs a baseline path"; exit 1; }
    printf 'SECTIONS=%s\nLINES=%s\n' "$SECTIONS" "$LINES" > "$BASE"
    echo "BASELINE CAPTURED -> $BASE (SECTIONS=$SECTIONS LINES=$LINES)"
    ;;
  --against)
    [ -f "$BASE" ] || { echo "no baseline: $BASE"; exit 1; }
    ;;
  *)
    echo "unknown MODE: '$MODE' (need --capture|--against)"; exit 1
    ;;
esac

if [ "$MODE" = "--against" ]; then
  B_SECTIONS=$(sed -n 's/^SECTIONS=//p' "$BASE")
  B_LINES=$(sed -n 's/^LINES=//p' "$BASE")
  NOW_S=$(grep -c '^## ' "$FILE")
  NOW_L=$(wc -l < "$FILE" | tr -d ' ')
  echo "THRESHOLD sections $NOW_S <= $B_SECTIONS ; lines $NOW_L <= $B_LINES"
  [ "$NOW_S" -le "$B_SECTIONS" ] || { echo "FAIL sections grew"; tfail=$((tfail+1)); }
  [ "$NOW_L" -le "$B_LINES" ]      || { echo "FAIL lines grew";    tfail=$((tfail+1)); }
fi

echo "ANCHORS=$(( ${#POLARITY[@]} + ${#DELETION[@]} )) ANCHOR_MISSING=$amiss THRESHOLD_FAILS=$tfail"
echo "NOTE: deletion 档只防删除不防翻义，须人工过语义（见 prd.md A2）"
[ "$((amiss + tfail))" -eq 0 ] || exit 1
if [ "$MODE" = "--capture" ]; then echo "VERDICT=BASELINE"; else echo "VERDICT=GREEN"; fi
