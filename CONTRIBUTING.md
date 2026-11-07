# 贡献指南

## 开发依据

SDK 与打包工具固定官方 [v3.41.2](https://github.com/zyycn/codex-proxy-rs/releases/tag/v3.41.2) 提交 `ba3baddbda7293d1d76ea066dd5e42298618c15a`，SDK crate 版本为 0.1.0，UI 使用官方 v0.3.0 发行包
宿主最低版本为 3.41.2，不设置版本上限；所需合同为 RPC v7 / Management v2 / Maintenance v1，升级 SDK 时须同时核对最低宿主与打包工具，不能仅凭宿主版本号推断合同兼容
管理页入口（开发 HTML 和打包生成的经典脚本 HTML）必须声明 `<meta name="codex-proxy-ui-theme-prefix" content="cp">`，与当前官方 UI v0.3.0 和页面 CSS 的 `--cp-*` 前缀保持一致。CPR v3.37.2 的主题桥接默认使用 `z`，未声明会导致颜色、表格背景及组件主题变量失效；旧宿主忽略该元数据，不需为此升级后端 SDK 或改变最低宿主版本。
先阅读 [设计合同](docs/design.md)，周期判定与重置语义属于数据安全边界

使用 Rust 1.99.1、Node.js 24 和 pnpm 12.6.0

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
pnpm --dir frontend install --no-frozen-lockfile
pnpm --dir frontend lint
pnpm --dir frontend test
pnpm --dir frontend build
```

联调必须在专用官方宿主及专用 PostgreSQL、Redis 中进行，使用合成账号与额度响应
通过真实 SDK、插件进程、安装器和管理页面验证，不以直接改库代替业务链路
响应丢失和进程退出可在传输边界定向注入，说明直接验证与推断范围

`cargo build --example fault-proxy` 构建仅用于隔离测试的传输代理，不进入发行包
代理读取 `/app/.runtime/data/follow-fault.json` 的 `binary` 和 `marker` 路径，启动被测插件
第一次收到真实宿主的 `host.keys.reset_budget` 回执时，创建一次性 marker 并中止进程，不将回执交给插件
随后确认插件恢复为“结果未知”，再次产生消费并检查其未被自动清零
在宿主使用前核对测试代理及被测二进制的平台和系统库兼容性；禁止在生产使用故障代理

## 提交与 PR

功能分支向 main 提 PR，检查通过后 squash 合并
提交使用英文 Conventional Commits，文档使用简体中文
PR 说明问题、行为、实际验证和未验证部分
使用 AI 时披露工具和实际模型型号，未知时如实注明并保持 Draft
不得上传真实账号、令牌、生产地址和生产截图

界面格式、文案与布局等细节不做测试；保留静态检查、构建和业务行为测试，真实宿主联调仍需隔离环境。

## 打包与发布

先构建插件二进制与页面，再从固定宿主提交构建 `codex-proxy-plugin-cli`

```sh
cargo build --release --locked
cpr-plugin package --manifest plugin.json --binary target/release/key-reset-follow \
  --target x86_64-unknown-linux-gnu --resource-map web=frontend/dist --output-dir dist
```

target 必须匹配实际编译平台，不通过修改平台参数伪装交叉编译
插件标识为 `tumi.key-reset-follow`，归档名为 `tumi.key-reset-follow-<version>-<target>.tar.gz`，发布工作流按同一标识核对附件
版本修改经 PR 合并，`v<version>` 标签与 plugin.json 逐字一致，只标记已验证提交
CI 仅通过 GitHub Actions 的 `workflow_dispatch` 手动触发，不随 push、PR 或标签推送自动运行。选择分支运行仅执行双架构构建和检查；需要发布时，先在 GitHub Releases **手动创建 Release 和 `v<version>` 标签**，确保标签指向本次待发布提交，再到 Actions → CI → Run workflow **选择该标签**运行（也可用 `gh workflow run ci.yml --ref v<version>`）。版本须与 `plugin.json` 一致。检查和 SHA-256 校验通过后，工作流只向已存在的 Release 上传附件，**同名附件允许覆盖**；不会自动创建 Release 或标签。CI 在对应架构的 Ubuntu 24.04 runner 上构建 Linux x86_64 与 ARM64 的 tar.gz 和 SHA-256 校验文件
已发布标签不移动；允许覆盖同名附件以修复缺失或错误的发布附件，业务变更应发布新版本
从预发行升级稳定版使用单独版本 PR，生产安装和稳定版发布须有对应用户授权
下载公开附件并核对摘要后才安装；相同插件标识升级时检查原有关联与状态保留，不同标识按独立安装验证，先停用旧插件且不假定状态自动迁移，详见 [安装说明](README.md#安装)
