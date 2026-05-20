# qqmail-rmcp

## 功能介绍

qqmail-rmcp 是一个本地运行的 QQ 邮箱 MCP 服务，基于 Rust 和 `rmcp` 提供 Streamable HTTP 接入。服务通过 `.env` 绑定单个 QQ 邮箱账号，使用 QQ 邮箱授权码连接 SMTP/IMAP，并通过 Bearer Token 保护 `/mcp` 接口。

它适合把 QQ 邮箱接入支持 MCP 的客户端，实现发送邮件、读取邮件、管理邮件状态等操作。

## mcp工具介绍

| 工具 | 说明 |
|---|---|
| `send_email` | 通过 QQ SMTP 发送邮件，支持收件人、抄送、密送、主题、纯文本和 HTML 正文。 |
| `list_mailboxes` | 列出当前 QQ 邮箱账号下可用的邮箱目录。 |
| `list_messages` | 分页列出指定邮箱目录中的邮件摘要，包含 UID、发件人、收件人、主题、日期和标记。 |
| `get_message` | 按 UID 读取单封邮件的完整内容，默认不标记为已读。 |
| `delete_message` | 按 UID 删除邮件，可选择是否立即 expunge。 |
| `move_message` | 按 UID 将邮件从一个邮箱目录移动到另一个目录。 |
| `mark_message` | 按 UID 更新邮件标记，支持已读、星标和已回复状态。 |

## 使用教程

复制环境变量示例并填写 QQ 邮箱配置：

```bash
cp .env.example .env
```

`.env` 中必须配置：

```env
QQMAIL_ADDRESS=your_qq_email@qq.com
QQMAIL_AUTH_CODE=your_authorization_code
MCP_ACCESS_TOKEN=your_secret_access_token_here
```

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
