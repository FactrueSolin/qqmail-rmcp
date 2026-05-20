# qqmail-rmcp

## 功能介绍

qqmail-rmcp 是一个本地运行的 QQ 邮箱 MCP 服务，基于 Rust 和 `rmcp` 提供 Streamable HTTP 接入。服务通过 YAML 管理多个 QQ 邮箱账号，使用 QQ 邮箱授权码连接 SMTP/IMAP，并通过 Bearer Token 保护 `/mcp` 接口。

它适合把 QQ 邮箱接入支持 MCP 的客户端，实现发送邮件、读取邮件、管理邮件状态等操作。

## mcp工具介绍

| 工具 | 说明 |
|---|---|
| `send_email` | 通过指定 `account` 的 QQ SMTP 发送邮件，支持收件人、抄送、密送、主题、纯文本和 HTML 正文。 |
| `list_mailboxes` | 列出指定 `account` 下可用的邮箱目录。 |
| `list_messages` | 分页列出指定 `account` 和邮箱目录中的邮件摘要，包含 UID、发件人、收件人、主题、日期和标记。 |
| `get_message` | 按 `account`、邮箱目录和 UID 读取单封邮件的完整内容，默认不标记为已读。 |
| `delete_message` | 按 `account`、邮箱目录和 UID 删除邮件，可选择是否立即 expunge。 |
| `move_message` | 按 `account` 将邮件从一个邮箱目录移动到另一个目录，不支持跨账号移动。 |
| `mark_message` | 按 `account`、邮箱目录和 UID 更新邮件标记，支持已读、星标和已回复状态。 |

## 使用教程

推荐复制 YAML 示例并填写多个 QQ 邮箱账号：

```bash
cp config/qqmail.yaml.example config/qqmail.yaml
```

`config/qqmail.yaml` 中必须配置：

```yaml
mcp:
  bind: 127.0.0.1:3000
  access_token: your_secret_access_token_here

mail:
  accounts:
    personal:
      provider: qq
      address: your_personal@qq.com
      auth_code: your_qq_authorization_code
```

也可以用 `QQMAIL_CONFIG=/path/to/qqmail.yaml` 指定配置文件。若没有 YAML，服务会兼容读取旧 `.env`，并归一化为 `default` 账号；调用 MCP tool 时仍必须显式传入 `account: "default"`。

启动服务：

```bash
cargo run
```

默认服务地址为：

```text
http://127.0.0.1:3000/mcp
```

MCP 客户端连接时需要携带 Bearer Token：

```json
{
  "mcpServers": {
    "qqmail": {
      "url": "http://127.0.0.1:3000/mcp",
      "headers": {
        "Authorization": "Bearer your_secret_access_token_here"
      }
    }
  }
}
```

所有 mail tool 都必须显式传入 `account`，不存在默认账号或隐式路由。例如：

```json
{
  "account": "personal",
  "mailbox": "INBOX",
  "limit": 20
}
```
