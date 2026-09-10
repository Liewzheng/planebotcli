# pbot

[English](cli-command-reference.md) · **中文**

## 名称

`pbot` —— [Plane.so](https://plane.so)（SaaS 或自托管）的命令行客户端：工作项（work item）、项目、周期、模块、标签、状态、文档、收件箱与评论。

`pbot` 是 `planebotcli` 的别名，两者是同一个二进制。

## 用法

```
pbot [全局选项] <命令> [<子命令>] [<参数>...] [选项]
```

## 说明

所有资源参数都支持**模糊解析**：可传名称、标识符（`ABC-123`）或 UUID；相近名称按相似度匹配（token-sort-ratio，阈值 60）。需要指定负责人处可用 `me` 表示当前用户。

人读表格输出到 **stderr**，机读 JSON 输出到 **stdout**，因此 `pbot ... --json 2>/dev/null` 总能得到纯 JSON。读操作走磁盘缓存；写操作会使对应缓存失效；对于 API 可能"返回成功但实际未生效"的写入（日期、triage、上传），会回读校验。

## 全局选项

| 选项 | 说明 |
|---|---|
| `--json` | JSON 输出到 stdout；人读表格到 stderr。 |
| `--no-cache` | 本次命令绕过磁盘缓存。 |
| `-v, --verbose` | 输出详细日志到 stderr。 |
| `-h, --help` | 查看任意命令的帮助。 |
| `-V, --version` | 打印版本号。 |

## 环境变量

| 变量 | 说明 |
|---|---|
| `PLANE_BASE_URL` | 实例地址，如 `https://api.plane.so`。 |
| `PLANE_API_KEY` | 服务令牌（`plane_api_...`）。 |
| `PLANE_WORKSPACE` | 工作区 slug（不是实例名）。 |

配置文件 `~/.plane_api`：`key=value` 文本，键为小写 `base_url`、`api_key`、`workspace`，`chmod 600`。优先级：命令行参数 > 环境变量 > `~/.plane_api`。

> 自托管实例若在代理后面，请这样运行，让 CLI 请求直连实例：`env -u http_proxy -u https_proxy -u HTTP_PROXY -u HTTPS_PROXY -u all_proxy pbot ...`

## 退出码

| 码 | 含义 |
|---|---|
| 0 | 成功 |
| 1 | 通用错误 |
| 2 | 认证失败 |
| 3 | 资源不存在 |
| 4 | API 错误（含 HTTP 状态码与字段级明细） |
| 5 | 校验错误（客户端侧） |

## 命令索引

| 命令 | 用途 |
|---|---|
| [`whoami`](#pbot-whoami) | 查看当前登录用户。 |
| [`configure`](#pbot-configure) | 交互式写入 `~/.plane_api`。 |
| [`user`](#pbot-user) | 工作区成员。 |
| [`cache`](#pbot-cache) | 本地磁盘缓存管理。 |
| [`project`](#pbot-project) | 项目。 |
| [`wi`](#pbot-wi) | 工作项（别名：`work-item`、`issues`、`issue`）。 |
| [`relations`](#pbot-relations) | 工作项关系（别名：`relation`）。 |
| [`comment`](#pbot-comment) | 工作项评论。 |
| [`label`](#pbot-label) | 标签。 |
| [`state`](#pbot-state) | 状态。 |
| [`module`](#pbot-module) | 模块。 |
| [`cycle`](#pbot-cycle) | 周期（Sprint）。 |
| [`intake`](#pbot-intake) | 收件箱队列。 |
| [`doc`](#pbot-doc) | 文档 / 页面。 |
| [`attachment`](#pbot-attachment) | 工作项附件（别名：`attachments`）。 |

---

## pbot whoami

查看当前登录用户。

```
pbot whoami [--json]
```

## pbot configure

交互式写入凭证到 `~/.plane_api`（依次提示实例地址、API Key、工作区 slug），随后清空磁盘缓存。

```
pbot configure
```

## pbot user

### pbot user list

列出工作区成员。

**别名** `ls`

```
pbot user list [--json]
```

## pbot cache

### pbot cache clear

清空本地磁盘缓存。

```
pbot cache clear
```

---

## pbot project

### pbot project list

列出工作区内的项目。

**别名** `ls`

```
pbot project list [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `--state <state>` | 按项目状态过滤。 |
| `--sort <field>` | 排序字段：`created` 或 `linear`（默认 `linear`）。 |
| `-l, --limit <n>` | 最大条数（默认 50）。 |

### pbot project show

查看项目详情。

```
pbot project show <project> [--json]
```

### pbot project create

新建项目。

```
pbot project create <name> [-i <identifier>] [-d <description>] [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `-i, --identifier <id>` | 项目标识符（省略时按名称自动生成）。 |
| `-d, --description <text>` | 项目描述。 |

### pbot project update

更新项目。

```
pbot project update <project> [--name <name>] [-i <identifier>] [-d <description>] [--json]
```

### pbot project delete

删除项目。

```
pbot project delete <project>
```

---

## pbot wi

工作项管理。**组别名：** `work-item`、`issues`、`issue`。

### pbot wi list

列出工作项。省略 `-p` 时跨所有项目列出。

```
pbot wi list [-p <project>] [--assignee <name|me>] [--state <s>] [--labels <a,b>]
             [--parent <id>] [--sort <field>] [-l <n>] [--json]
```

**别名** `ls`

**选项**

| 选项 | 说明 |
|---|---|
| `-p, --project <name\|id>` | 项目名称、标识符或 UUID；省略则跨全部项目。 |
| `--assignee <name\|me>` | 按负责人过滤。 |
| `--state <s>` | 按状态名过滤（逗号分隔）。 |
| `--labels <a,b>` | 按标签名过滤（逗号分隔）。 |
| `--parent <id>` | 列出某父项的子工作项（`ABC-123`、UUID 或名称）。 |
| `--sort <field>` | `created`（默认）或 `updated`。 |
| `-l, --limit <n>` | 最大条数（默认 50）。 |

**示例**

```
$ pbot wi ls -p PLANECLI --state "In Progress" --assignee me --json
$ pbot wi ls -p PLANECLI --parent PLANECLI-38 --json
```

### pbot wi show

查看工作项详情。JSON 含 `web_url`、`parent` 与 `sub_issues`。

```
pbot wi show <issue> [-p <project>] [--no-comments] [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `-p, --project <name\|id>` | 项目名称/ID（按名称查找时必填）。 |
| `--no-comments` | 跳过拉取评论。 |

### pbot wi create

新建工作项。

**别名** `new`

```
pbot wi create <title> [-p <project>] [--assign <name|me>] [--state <s>]
              [--labels <a,b>] [--priority <p>] [--parent <id>]
              [-d <text> | --desc-md <markdown>] [--start-date <date>]
              [--target-date <date>] [-i <file>...] [--force] [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `-p, --project <name\|id>` | 项目（必填）。 |
| `--assignee, --assign <name\|me>` | 负责人。 |
| `--state <s>` | 状态名（写入前按项目校验）。 |
| `--labels <a,b>` | 逗号分隔的标签名（写入前按项目校验）。 |
| `--priority <p>` | `urgent`、`high`、`medium`、`low`、`none`（或 `1`–`4`、`0`）。 |
| `--parent <id>` | 父工作项（`ABC-123`、UUID 或名称）。 |
| `-d, --description <text>` | 描述（纯文本，包一层段落）。 |
| `--desc-md <markdown>` | 描述（原生 markdown：标题/列表/代码/链接）。与 `-d` 互斥。 |
| `--start-date <YYYY-MM-DD>` | 开始日期（格式校验，写入后回读确认）。 |
| `--target-date <YYYY-MM-DD>` | 截止日期。 |
| `-i, --image <file>` | 上传图片并嵌入描述（可重复）。 |
| `--force` | 同名附件已存在时仍上传图片。 |

**示例**

```
$ pbot wi create "修复登录" -p PLANECLI --assign me --state Todo --priority high --json
$ pbot wi create "子任务" -p PLANECLI --parent PLANECLI-9 --desc-md "# 背景

- 要点" --json
```

### pbot wi update

更新工作项。日期与父项的写入会回读校验。

```
pbot wi update <issue> [-p <project>] [--state <s>] [--priority <p>]
              [--assign <name|me>] [--labels <a,b>] [--clear-labels]
              [--name <title>] [-d <text> | --desc-md <markdown>]
              [--start-date <date>] [--target-date <date>]
              [--parent <id>] [--clear-parent] [-i <file>...] [--force] [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `-p, --project <name\|id>` | 项目（按名称查找时需要）。 |
| `--state <s>` | 新状态（校验，缺失时列出可用项）。 |
| `--priority <p>` | 新优先级。 |
| `--assignee, --assign <name\|me>` | 新负责人。 |
| `--labels <a,b>` | 设置标签（逗号分隔）。 |
| `--clear-labels` | 清空全部标签。 |
| `--name <title>` | 新标题。 |
| `-d, --description <text>` | 新描述（纯文本）。 |
| `--desc-md <markdown>` | 新描述（markdown）。与 `-d` 互斥。 |
| `--start-date <date>` / `--target-date <date>` | 新日期。 |
| `--parent <id>` | 设置父项（自引用/跨项目/不存在会被本地拒绝）。 |
| `--clear-parent` | 解除父项。 |
| `-i, --image <file>` | 上传图片并追加到描述（可重复）。 |
| `--force` | 允许同名图片重复上传。 |

**示例**

```
$ pbot wi update PLANECLI-40 --state "In Progress" --json
$ pbot wi update PLANECLI-40 --parent PLANECLI-38 --json
$ pbot wi update PLANECLI-40 --clear-parent --json
```

### pbot wi delete

删除工作项。

```
pbot wi delete <issue> [-p <project>]
```

### pbot wi search

按文本搜索工作项。

```
pbot wi search <query> [-p <project>] [-l <n>] [--json]
```

### pbot wi assign

指派工作项（默认指派给自己）。

```
pbot wi assign <issue> [--assign <name|me>] [-p <project>]
```

**另见** `pbot relations`、`pbot comment`

---

## pbot relations

工作项关系管理。**组别名：** `relation`。

关系类型：`blocking`、`blocked_by`、`duplicate`、`relates_to`、`start_before`、`start_after`、`finish_before`、`finish_after`。

### pbot relations list

列出某工作项的关系（八个分类桶）。

**别名** `ls`

```
pbot relations list <issue> [-p <project>] [--json]
```

### pbot relations add

为某工作项建立一条或多条关系。

```
pbot relations add <issue> --type <type> --to <target>... [-p <project>] [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `--type <type>` | 八种关系类型之一。 |
| `-t, --to <target>` | 目标工作项（`ABC-123`、UUID 或名称）；可重复。 |

**示例**

```
$ pbot relations add PLANECLI-38 --type relates_to --to PLANECLI-40 -p PLANECLI --json
```

### pbot relations remove

**暂不可用** —— 后端缺少关系删除接口。该命令会以校验错误退出，并指向对应的后端需求。

---

## pbot comment

### pbot comment list

列出某工作项的评论（从旧到新）。

**别名** `ls`

```
pbot comment list <issue> [-p <project>] [-l <n>] [--json]
```

### pbot comment create

新增评论。

**别名** `new`

```
pbot comment create <issue> (-b <text> | --body-md <markdown>) [-p <project>] [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `-b, --body <text>` | 评论正文（纯文本；分段、`code`、裸链接会转成 HTML）。 |
| `--body-md <markdown>` | 评论正文（原生 markdown）。与 `-b` 互斥。 |

### pbot comment update

更新评论。

```
pbot comment update <comment-id> --issue <issue> (-b <text> | --body-md <markdown>) [--json]
```

### pbot comment delete

删除评论。

```
pbot comment delete <comment-id> --issue <issue> [-p <project>]
```

---

## pbot label

### pbot label list

列出项目中的标签。

**别名** `ls`

```
pbot label list -p <project> [--json]
```

### pbot label show

查看标签详情。

```
pbot label show <label> -p <project> [--json]
```

### pbot label create

新建标签。

```
pbot label create <name> -p <project> [--color <#RRGGBB>] [--json]
```

### pbot label update

更新标签。

```
pbot label update <label> -p <project> [--name <name>] [--color <#RRGGBB>] [--json]
```

### pbot label delete

删除标签。

```
pbot label delete <label> -p <project>
```

---

## pbot state

### pbot state list

列出项目中的状态。

**别名** `ls`

```
pbot state list -p <project> [--group <group>] [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `--group <group>` | 按分组过滤：`backlog`、`unstarted`、`started`、`completed`、`cancelled`。 |

### pbot state show

查看状态详情。

```
pbot state show <state> -p <project> [--json]
```

### pbot state create

新建状态。

```
pbot state create <name> -p <project> [--group <group>] [--color <#RRGGBB>] [--json]
```

### pbot state update

更新状态。

```
pbot state update <state> -p <project> [--name <name>] [--group <group>] [--color <#RRGGBB>] [--json]
```

### pbot state delete

删除状态。

```
pbot state delete <state> -p <project>
```

---

## pbot module

### pbot module list

列出项目中的模块。

**别名** `ls`

```
pbot module list -p <project> [--json]
```

### pbot module show

查看模块详情。

```
pbot module show <module> -p <project> [--json]
```

### pbot module create

新建模块。

```
pbot module create <name> -p <project> [-d <text>] [--start-date <date>]
                    [--end-date <date>] [--status <status>] [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `--status <status>` | `backlog`、`planned`、`in-progress`、`paused`、`completed`、`cancelled`。 |

### pbot module update

更新模块。

```
pbot module update <module> -p <project> [--name <name>] [-d <text>] [--status <status>]
                    [--start-date <date>] [--end-date <date>] [--json]
```

### pbot module delete

删除模块。

```
pbot module delete <module> -p <project>
```

---

## pbot cycle

### pbot cycle list

列出项目中的周期。

**别名** `ls`

```
pbot cycle list -p <project> [--json]
```

### pbot cycle show

查看周期详情。

```
pbot cycle show <cycle> -p <project> [--json]
```

### pbot cycle create

新建周期。

```
pbot cycle create <name> -p <project> [-d <text>] [--start-date <date>] [--end-date <date>] [--json]
```

### pbot cycle update

更新周期。

```
pbot cycle update <cycle> -p <project> [--name <name>] [--start-date <date>] [--end-date <date>] [--json]
```

### pbot cycle delete

删除周期。

```
pbot cycle delete <cycle> -p <project>
```

### pbot cycle add-item

把工作项加入周期。

```
pbot cycle add-item <cycle> <issue> -p <project>
```

### pbot cycle remove-item

把工作项移出周期。

```
pbot cycle remove-item <cycle> <issue> -p <project>
```

### pbot cycle items

列出周期内的工作项。

```
pbot cycle items <cycle> -p <project> [--json]
```

---

## pbot intake

### pbot intake list

列出项目的收件箱队列。每行含队列包装 `id` 与工作项 `issue_id`。

**别名** `ls`

```
pbot intake list -p <project> [--json]
```

### pbot intake create

新建收件箱条目。

**别名** `new`

```
pbot intake create <name> -p <project> [-d <text>] [-P <priority>] [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `-d, --description <text>` | 描述（会做 HTML 转义，标签按文本显示）。 |
| `-P, --priority <priority>` | 优先级（`none`、`low`、`medium`、`high`、`urgent`）。 |

### pbot intake accept

接受收件箱条目（triage）。需项目 Admin 角色；CLI 会校验状态是否真的变化，未生效则报错。

```
pbot intake accept <issue-id> -p <project> [--json]
```

### pbot intake decline

拒绝收件箱条目（triage）。需项目 Admin 角色。

```
pbot intake decline <issue-id> -p <project> [--json]
```

### pbot intake delete

删除收件箱条目。**破坏性：** 状态不是 `accepted` 时，会连同底层工作项一起删除。

```
pbot intake delete <issue-id> -p <project>
```

### pbot intake enabled

查看项目是否启用收件箱。

```
pbot intake enabled <project> [--json]
```

---

## pbot doc

### pbot doc list

列出文档。必须带 `-p`（项目内页面）。

**别名** `ls`

```
pbot doc list -p <project> [--json]
```

### pbot doc show

查看文档详情。

**别名** `read`

```
pbot doc show <doc> [-p <project>] [--json]
```

### pbot doc create

新建文档。

**别名** `new`

```
pbot doc create --title <title> (--content <text> | --content-md <markdown> | --content-html <html>)
                [-p <project>] [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `--title <title>` | 页面标题（必填）。 |
| `-c, --content <text>` | 内容（纯文本，按评论方式转换：分段、`code`、链接）。 |
| `--content-md <markdown>` | 内容（原生 markdown：标题/列表/代码/链接）。 |
| `--content-html <html>` | 内容（原样 HTML，存原文，适合富排版）。 |
| `-p, --project <name\|id>` | 项目；省略则创建到工作区级页面。 |

三种内容输入互斥。

**示例**

```
$ pbot doc create --title "Runbook" -p PLANECLI --content-md "$(cat runbook.md)" --json
$ pbot doc create --title "Spec" -p PLANECLI --content-html "$(cat spec.html)" --json
```

### pbot doc update

更新文档。

```
pbot doc update <doc> [--title <title>] [--content <text> | --content-md <markdown> | --content-html <html>]
                [-p <project>] [--json]
```

### pbot doc archive

归档（进回收站）页面，不删除；页面在 Web 回收站可恢复。

```
pbot doc archive <doc> [-p <project>]
```

### pbot doc delete

删除页面。API 只允许删除已归档页面，因此本命令先归档（并回读确认归档生效）再删除。

```
pbot doc delete <doc> [-p <project>]
```

---

## pbot attachment

**组别名：** `attachments`。

### pbot attachment attach

上传附件到工作项（预签名三步上传，写后回读验证）。

**别名** `upload`、`new`

```
pbot attachment attach <issue> -f <file> [-p <project>] [--force] [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `-f, --file <path>` | 待上传文件。 |
| `--force` | 同名附件已存在时仍上传。 |

### pbot attachment list

列出工作项的附件。

**别名** `ls`

```
pbot attachment list <issue> [-p <project>] [--json]
```

---

## 另见

- `pbot <命令> <子命令> --help` —— 由 CLI 定义直接生成的逐命令帮助。
- Plane v1 API 参考：`docs/api/plane-v1-api.md`。
