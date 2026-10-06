#!/usr/bin/env bash
#
# 修复 LanMusic 的 GitHub Release 资产错位（v0.5.25 实录：安装包被挂到 untagged-<hash> release，
# 且 v0.5.26 草稿排在 releases.atom 首位，导致 0.5.24 等旧客户端永远收不到更新提示）。
#
# 这个脚本只动「GitHub Release / git tag」，不碰任何本地仓库文件、不动你的音乐。
# 需要在能访问 github.com 且 `gh` 已登录（gh auth status 通过）的终端里运行。
#
# 用法：
#   ./scripts/fix-github-releases.sh            # 交互式，每步确认
#   ./scripts/fix-github-releases.sh --yes       # 跳过「是否继续」确认（删除动作仍会逐个提示）
#   ./scripts/fix-github-releases.sh --dry-run   # 只打印将要执行的命令，什么都不改
#
set -euo pipefail

VERSION="${LM_FIX_VERSION:-0.5.25}"   # 想修别的版本就：LM_FIX_VERSION=0.5.26 ./scripts/fix-github-releases.sh
AUTO_YES=0
DRY_RUN=0
for a in "$@"; do
  case "$a" in
    --yes) AUTO_YES=1 ;;
    --dry-run) DRY_RUN=1 ;;
    *) echo "未知参数: $a" >&2; exit 1 ;;
  esac
done

REPO="$(gh repo view --json nameWithOwner -q .nameWithOwner 2>/dev/null)"
if [ -z "$REPO" ]; then
  echo "无法解析当前仓库（gh repo view 失败）。请在克隆的仓库根目录运行，且 gh 已登录。" >&2
  exit 1
fi
echo "目标仓库: $REPO"

run() {
  # 打印将要执行的命令；非 dry-run 时才真正执行
  echo "  \$ $*"
  if [ "$DRY_RUN" -eq 0 ]; then
    "$@"
  fi
}

confirm() {
  local msg="$1"
  if [ "$DRY_RUN" -eq 1 ]; then return 0; fi
  if [ "$AUTO_YES" -eq 1 ]; then return 0; fi
  local ans
  printf '%s [y/N] ' "$msg"
  read -r ans
  case "$ans" in
    y|Y|yes|YES) return 0 ;;
    *) echo "跳过。"; return 1 ;;
  esac
}

# 1) 找到真正持有该版本安装包的 release（通常是 untagged-<hash>）
echo
echo "==> 查找持有 LanMusic_${VERSION}_x64-setup.exe 的 release ..."
SRC_TAGS="$(gh api "repos/$REPO/releases" \
  --jq ".[] | select(.assets[].name | contains(\"LanMusic_${VERSION}_x64-setup.exe\")) | .tag_name" \
  | grep -v "^v${VERSION}$" || true)"

if [ -z "$SRC_TAGS" ]; then
  echo "没有找到错位的源 release（v${VERSION} 安装包似乎已在正确位置）。跳过下载/迁移。"
else
  echo "错位源 tag："
  echo "$SRC_TAGS" | sed 's/^/  - /'
  TMP="$(mktemp -d)"
  echo "临时目录: $TMP"

  while IFS= read -r src; do
    [ -z "$src" ] && continue
    echo
    echo "==> 处理源 release: $src"
    if confirm "下载 $src 的资产到临时目录？"; then
      run gh release download "$src" -R "$REPO" -D "$TMP"
    fi

    echo "==> 确保 v${VERSION} release 存在 ..."
    if ! gh release view "v${VERSION}" -R "$REPO" >/dev/null 2>&1; then
      if confirm "v${VERSION} release 不存在，是否创建草稿 release 并写入资产？"; then
        run gh release create "v${VERSION}" -R "$REPO" --draft --title "v${VERSION}" \
          --notes "安装包未签名：macOS 首次打开请右键→打开；Windows SmartScreen 提示选择「仍要运行」。"
        run gh release upload "v${VERSION}" -R "$REPO" "$TMP"/*
      fi
    else
      if confirm "往已有 v${VERSION} release 上传 Windows 资产（若存在则跳过）？"; then
        run gh release upload "v${VERSION}" -R "$REPO" "$TMP"/* || true
      fi
    fi

    if confirm "发布 v${VERSION}（--draft=false）？发布后就不能再往里传资产了。"; then
      run gh release edit "v${VERSION}" -R "$REPO" --draft=false
    fi

    if confirm "删除错位源 release: $src ？其资产已迁移到 v${VERSION} 后可安全删除。"; then
      run gh release delete "$src" -R "$REPO" -y
    fi
  done <<< "$SRC_TAGS"
fi

# 2) 处理排在最前面的 v0.5.26 等草稿：旧客户端只认 atom 第一条，草稿会挡住后面真正的更新
echo
echo "==> 检查可能挡路的草稿 release（releases.atom 首位若是草稿，旧客户端会卡死）..."
DRAFTS="$(gh api "repos/$REPO/releases" --jq ".[] | select(.draft) | .tag_name" || true)"
if [ -z "$DRAFTS" ]; then
  echo "没有草稿 release，OK。"
else
  echo "草稿 tag："
  echo "$DRAFTS" | sed 's/^/  - /'
  echo "说明：旧客户端（如 0.5.24）只读取 releases.atom 第一条。若草稿 tag 版本高于当前版本且排在首位，"
  echo "      旧客户端会因草稿资产 404 而直接判定「无更新」。建议删除尚未就绪的草稿，或在它就绪后正式发布。"
  while IFS= read -r d; do
    [ -z "$d" ] && continue
    if confirm "删除草稿 release: $d ？（若它其实已就绪、应发布，请选 N 后自行 gh release edit --draft=false）"; then
      run gh release delete "$d" -R "$REPO" -y
    fi
  done <<< "$DRAFTS"
fi

echo
echo "完成。请在 LanMusic 里点「检查更新」验证（旧客户端可能需要重启一次）。"
