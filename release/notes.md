# v0.1.4 — 跟随重置

适配官方 Codex Proxy RS v3.29.7（RPC v5 / Management v2 / Maintenance v1）。SDK 与打包 CLI 均固定官方 v3.29.7 发布提交 `78bc57c5cf24b01a38e985cdd739bb4ae89ce11f`，Rust 工具链升级至 1.99.1。

- 保持现有账号周额度周期判断、各密钥提前重置开关和清零安全保护，不更改私有状态 Schema。
- 使用新版 SDK 重新编译两个 Linux 架构的可安装插件包，提供 SHA-256 校验文件。
- 插件宿主范围改为 CPR >=3.29.7、<3.30.0，不支持 RPC v4 宿主。

## 验证与发布

需要在 GitHub Actions 手动运行双架构格式、Clippy、Rust 测试、前端测试与打包；本次源码变更尚不能代替实际宿主 v3.29.7 安装、升级及保留状态的联调。发布须先手动创建 v0.1.4 标签与 Release，再选择标签运行工作流上传附件。
