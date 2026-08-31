# LanMusic

本地 + 局域网音乐播放器（桌面端）。技术栈：**Tauri 2 + Vue 3 + Rust**。

> 产品设计文档见 [docs/产品设计文档.md](docs/产品设计文档.md)。
> 当前进度：**M0-M3 已完成**（本地播放闭环 / 歌单 / 歌词 / 最近播放 / 托盘 / mDNS 发现 / 共享模式 / WebDAV）。

## 快速开始

```bash
npm install          # 前端依赖
npm run tauri dev    # 开发模式（首次需编译 Rust，约 2-3 分钟）
npm run tauri build  # 打包安装程序
```

环境要求：Node 20+、Rust 1.85+（含各平台 WebView 运行时）。

## 已实现功能

### M1 本地播放闭环
- 本地音乐库：添加文件夹、增量扫描（mtime+size diff）、lofty 元数据解析
- 扫描性能：独立 SQLite 连接（WAL 读写分离）、多线程并发解析、封面惰性提取、枚举/解析双阶段进度
- 快速导入：针对网络目录的开关，仅按文件名/目录结构入库；「完整解析」随时补全标签
- `music://` 自定义流协议：HTTP Range 拖动进度，本地/远程统一路由，跨平台适配
- 播放：播放模式、队列管理、虚拟滚动列表（10 万级）、专辑/艺人/文件夹视图、搜索

### M2 库体验
- 歌单：新建/重命名/删除、右键加入、拖拽排序、播放全部
- 歌词：.lrc 同名文件 + 内嵌歌词；现在播放页大封面 + 时间轴滚动歌词（点击行跳转）
- 最近播放（play_count / last_played_at 统计）
- 系统托盘：播放暂停/切曲/显示窗口/退出

### M3 局域网
- 共享模式（服务端）：axum 内嵌 HTTP 服务，Bearer 配对码鉴权，只共享本地来源且只读
- mDNS 自动发现：`_lanmusic._tcp` 广播与浏览，设备列表实时更新
- LAN 来源：直接拉取对方设备元数据（零逐文件 I/O）、远程封面/歌词、经本机代理流播放
- WebDAV 源：PROPFIND 遍历、Range 拉文件头解析标签、外挂 lrc/封面 URL 记录
- 远程流统一代理：Rust 侧转发 Range（2MB 分块），凭证不出进程

## 目录结构

```
src/                  # Vue 3 前端
├── api/              # invoke 封装（与 Tauri 交互的唯一边界）
├── stores/           # Pinia：player（播放状态机）/ library（库数据）
├── components/       # PlayerBar / TrackTable(虚拟滚动) / QueuePanel ...
├── views/            # Tracks / Albums / Artists / Network(占位) / Settings
└── composables/      # useNav / useTheme / useToast

src-tauri/            # Rust 后端
└── src/
    ├── db.rs         # SQLite schema + 迁移
    ├── metadata.rs   # lofty 元数据解析
    ├── scanner.rs    # 增量扫描管线（后台线程 + 进度事件）
    ├── scheme.rs     # music:// 音频流协议（Range）、cover:// 封面协议
    ├── commands.rs   # IPC 命令层
    └── state.rs      # AppState
```

## 数据位置

- 数据库：`~/Library/Application Support/com.lanmusic.desktop/library.db`（macOS）
- 封面缓存：同目录 `covers/` 下，按专辑 ID 命名

## 路线图

- **M4**：系统媒体键（SMTC/MPRIS）、WebDAV 凭证入系统钥匙串、DLNA、目录监听（notify）、打包签名
- 已知取舍：WebDAV 标签解析基于文件头部 1MB（moov 在尾部的 M4A 可能缺时长）；歌词暂存原始文本，编辑器后续提供
