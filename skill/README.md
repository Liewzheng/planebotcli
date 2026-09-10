# pbot skill

让 AI 助手通过 [pbot / planebotcli](https://github.com/Liewzheng/planebotcli)（`pbot` 命令，Rust 版 CLI）
管理 Plane.so（SaaS 或自托管）上的 work item、project、cycle、module、label、state、document、intake、comment。

> `pbot` 是 Rust 版 CLI；仓库里原先的 Python 实现已移除，不再是可选项。

## 这个仓库里有什么

- `SKILL.md` — 技能主体。AI 每次会话自动加载它，里面的 `description` 告诉 AI 什么时候该触发
  （用户提到 Plane、pbot、`ABC-123` 这类 work item 编号时）。
- `references/command-reference.md` — 全部命令的完整参数手册，AI 在需要精确 flag 时查阅。

## 前置条件（必须先满足）

1. **安装 pbot**（任一途径）：

   ```bash
   # 从源码（integration-main 检出后）
   cargo install --path crates/planebotcli-cli --locked

   # 或 GitHub Releases 直接下载/一键脚本
   # https://github.com/Liewzheng/planebotcli/releases
   ```

2. **配置凭据**，写入任一被发现的位置（按优先级：`~/.config/pbot/config.toml` → `~/.pbot` → `~/.planecli` → `~/.plane_api`）或导出环境变量：

   ```bash
   cat > ~/.plane_api <<EOF
   base_url=http://YOUR_PLANE_HOST    # 自托管实例地址，SaaS 用 https://api.plane.so
   api_key=plane_api_xxxxxxxx         # Plane Web 界面 → Workspace Settings → API tokens
   workspace=your-workspace           # workspace slug，不是实例名
   EOF
   chmod 600 ~/.plane_api
   ```

   验证：`pbot whoami` 能显示当前用户即配置成功。自托管实例的排障方法见 SKILL.md 的
   "Setup & Authentication" 一节。

## 安装 skill（AI 侧）

skill 的本质是一个含 `SKILL.md` 的目录，放进 AI 的 skill 搜索路径即可。本目录随
`github.com/Liewzheng/planebotcli` 仓库维护（`skill/` 子目录），安装方式：

```bash
# Kimi Code 用户级（对所有项目生效）
git clone git@github.com:Liewzheng/planebotcli.git /tmp/planebotcli
cp -r /tmp/planebotcli/skill ~/.kimi-code/skills/pbot

# 或项目级（只对该仓库生效，可进版本库共享给团队）
git submodule add https://github.com/Liewzheng/planebotcli.git skills/planebotcli

# 开发调试时用软链代替复制，改完即生效
ln -s ~/Workspace/github.com/planebotcli/skill ~/.kimi-code/skills/pbot
```

其他 Agent（Claude Code、Cursor 等）同理，把 `skill` 目录放到各自的 skills 路径
（如 `~/.claude/skills/pbot`）。

## 新 AI 上手检查单

1. `pbot whoami --json` 确认凭据可用；
2. `pbot project ls --json` 确认能读项目；
3. 读 `SKILL.md` 的 Key Concepts（模糊解析、`--json`、缓存行为）；
4. 需要具体 flag 时查 `references/command-reference.md`，不要猜。

## 更新

```bash
cd ~/.kimi-code/skills/pbot && git pull   # 若用 planebotcli 子模块方式
# 若为复制方式：重新 cp -r /tmp/planebotcli/skill ~/.kimi-code/skills/pbot
```
