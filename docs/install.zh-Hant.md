# 安裝與部署

本頁是三種交付形態的**完整安裝與維運說明**：桌面端（Windows / macOS / Linux）、伺服器端（Windows / Linux，無介面）、Docker。不確定該選哪個，先看下面的對照表。

各形態裝完之後**都在瀏覽器面板裡管理**（加帳號、建閘道 Key、看報表）—— 面板與閘道同連接埠，桌面端則額外有應用程式視窗。

[简体中文](./install.md) | [English](./install.en.md) | **繁體中文** | [日本語](./install.ja.md) | [한국어](./install.ko.md) | [Português (BR)](./install.pt-BR.md)

## 目錄

- [選哪種形態](#選哪種形態)
- [桌面端（Windows / macOS / Linux）](#桌面端windows--macos--linux)
- [伺服器端（Windows / Linux，無介面）](#伺服器端windows--linux無介面)
- [Docker](#docker)
- [驗證與客戶端接入](#驗證與客戶端接入)
- [區域網路存取](#區域網路存取)

## 選哪種形態

依你的機器挑一行：

| 形態 | 產物 | 介面 | 說明 |
| --- | --- | --- | --- |
| **Windows 桌面端**（推薦） | `Agent2API_<版本>_x64-setup.exe` | 應用程式視窗 | NSIS 安裝包，預設裝到 `C:\Program Files\Agent2API`，安裝時需要管理員授權 |
| **macOS 桌面端** | `Agent2API_<版本>_universal.dmg` | 應用程式視窗 | 通用二進位（Apple Silicon 與 Intel 各編一遍再合併），最低 macOS 10.15 |
| **Linux 桌面端** | `Agent2API_<版本>_amd64.deb` 或 `Agent2API_<版本>_amd64.AppImage` | 應用程式視窗 | deb 裝進系統（依賴 webkit2gtk-4.1 / gtk3）；AppImage 免安裝單檔，任何發行版可用 |
| **Windows 伺服器端**（無 GUI） | `agent2api-server-windows-amd64.zip` | **瀏覽器面板** | 命令列安裝、註冊成 Windows 服務，見 [伺服器端](#伺服器端windows--linux無介面) |
| **Linux 伺服器端**（無 GUI） | `agent2api-server-linux-<arch>.tar.gz` | **瀏覽器面板** | 命令列安裝、systemd 代管；同一個包也是容器內更新載荷 |
| **Docker** | `aimodcc/agent2api:latest` | **瀏覽器面板** | amd64 / arm64 雙架構，見 [Docker 部署](#docker) |

> 選哪個：**自己有桌面**（要系統匣、開機自動啟動、本機登入狀態匯入）→ 桌面端；
> **伺服器上沒有圖形介面**（Windows Server / Linux 伺服器）→ 伺服器端包；
> **已經跑 Docker** → 映像檔。
>
> 桌面端需要圖形介面，**裝不到無 GUI 的伺服器上**。伺服器端包不連結任何
> WebView / GTK 依賴，一個執行檔加一個 `ui/` 目錄就能跑。

## 桌面端（Windows / macOS / Linux）

**桌面端**（Windows / macOS / Linux）安裝後啟動即可，**無需安裝 Node 或任何其它執行環境**；閘道跑在應用程式行程內，常駐系統匣。

1. 首次啟動即在應用程式行程內啟動本機閘道（連接埠 3065）並開啟主視窗；若偵測到舊版本的資料目錄或資料檔案，會彈窗提示遷移，依指引操作即可。
2. 點「帳號」頁的「新增帳號」，選供應商後依彈窗提示完成登入或填寫憑證即可（方式以彈窗為準：網頁登入 / 手機驗證碼 / 貼上憑證 / 匯入本機登入狀態）。
3. 把 OpenAI 客戶端的 `base_url` 填成 `http://127.0.0.1:3065/v1`，`api_key` 隨便填（例如 `sk-local`，未啟用鑑權時伺服器端不校驗）。

關閉視窗預設最小化到系統匣，閘道繼續在背景轉發；徹底結束請在系統匣圖示按右鍵選「結束」。

> **macOS 首次開啟**：dmg 裡的應用程式未做 Apple 程式碼簽章與公證（簽章需要付費的 Apple Developer 帳號，見 `.github/workflows/build.yml` 的說明），Gatekeeper 會攔下首次啟動 —— 到「系統設定 → 隱私權與安全性」手動放行一次即可。
>
> **Linux 桌面端**：系統匣依賴 `libayatana-appindicator3`（多數桌面環境已內建）；若圖示不顯示，裝 `libayatana-appindicator3-1` 後重開即可。AppImage 首次執行前需要可執行位（`chmod +x`，程式在「開啟安裝包」時會自動補）。更新方式：deb 走系統安裝器（安裝時會問一次密碼），AppImage 是換檔案即升級。
>
> **桌面端的差異**：AutoClaw 與智譜清言的「匯入本機登入狀態」只在 Windows 可用（讀的是 Windows 的 DPAPI 密文，macOS / Linux 上整段收起）；其餘家（小浣熊 / CatPaw / Cline）讀的是 `HOME` 下的明文檔案，三個平台都可用。系統匣、開機自動啟動、區域網路存取與網頁面板代管在 macOS / Linux 上同樣支援。

## 伺服器端（Windows / Linux，無介面）

給**沒有圖形介面的伺服器**用（Windows Server、Linux 伺服器、雲端主機）。產物是同一份
`agent2api-server` 二進位檔：無視窗、無系統匣、不連結 WebView / GTK，執行時不依賴
Node 或任何其它執行環境。指令只負責**安裝**，裝完之後的管理（加帳號、建 Key、
看報表）都在瀏覽器面板裡做。

服務以系統服務方式常駐：開機自動啟動、崩潰自動拉起、無主控台在背景下執行。

### Windows Server

下載 `agent2api-server-windows-amd64.zip`，解壓後**以系統管理員身分**開啟 PowerShell：

```powershell
Expand-Archive agent2api-server-windows-amd64.zip -DestinationPath C:\agent2api-pkg
cd C:\agent2api-pkg
.\install-service.ps1
```

腳本會：把程式裝到 `C:\Agent2API`、資料落到 `C:\ProgramData\Agent2API`、註冊名為
`Agent2API` 的 Windows 服務（`LocalSystem`，開機自動啟動）、加入防火牆輸入規則，然後啟動。

```powershell
# 自訂位置與連接埠
.\install-service.ps1 -InstallDir D:\apps\agent2api -DataDir D:\data\agent2api -Port 8080

# 日常管理
Get-Service Agent2API
Restart-Service Agent2API
Stop-Service Agent2API

# 解除安裝（保留資料）；加 -Purge 連程式與資料一起刪
.\install-service.ps1 -Uninstall
```

**升級**：解壓新版本包，重跑一次 `install-service.ps1` 即可（腳本會先停服務、
覆蓋程式與介面、再啟動；資料與設定不動）。

> **兩個 Windows 特有的坑，腳本已經處理好了**：
> 服務行程的工作目錄是 `C:\Windows\system32` 而不是 exe 所在目錄 —— 介面目錄靠
> exe 路徑定位，不受影響；資料目錄則由腳本明確寫入服務登錄檔的
> `AGENT2API_PROXY_HOME`（不設的話會落到 LocalSystem 的
> `C:\Windows\system32\config\systemprofile` 下，幾乎不可能找到）。
> 服務沒有主控台，所以日誌同時落檔案 `C:\ProgramData\Agent2API\logs\server.log`。

### Linux（systemd）

```bash
tar -xzf agent2api-server-linux-amd64.tar.gz
cd agent2api-server-linux-amd64      # 或解包後的目錄名
sudo bash install.sh
```

腳本會：建 `agent2api` 系統帳號（不允許登入）、裝程式到 `/opt/agent2api`、資料落
`/var/lib/agent2api`、設定寫到 `/etc/agent2api/agent2api.env`、裝 systemd 單元並
`enable + start`。

```bash
# 升級（保留資料與設定，舊二進位檔備份為 .bak）
sudo bash install.sh --upgrade

# 解除安裝（保留資料）；加 --purge 連資料與設定一起刪
sudo bash install.sh --uninstall

# 日常管理
systemctl status agent2api
systemctl restart agent2api
journalctl -u agent2api -f
```

開機自動啟動、崩潰自動拉起（`Restart=always`）、SIGTERM 優雅停機（等在途請求跑完
再結束，SSE 長連線不會被打斷）都已配好。設定改 `/etc/agent2api/agent2api.env`
後 `systemctl restart agent2api` 生效。

> **裸機不要開 `AGENT2API_SELF_UPDATE`**：那是容器內的方案（換檔案後結束，靠
> 容器 restart 策略拉起）。裸機上程式檔案屬 root、服務帳號無權替換，單元裡已
> 明確設成 `0`。升級走 `install.sh --upgrade`。

### 裝完之後

兩個平台都一樣 —— 開啟瀏覽器：

```
http://<伺服器IP>:3065/
```

1. 首次進入會引導**註冊管理員帳號**（也可以用環境變數預置，見下）
2. 登入後在「閘道 Key」頁建立一把 API Key
3. 客戶端 `base_url` 填 `http://<伺服器IP>:3065/v1`，`api_key` 填剛建的那把

> ⚠️ **建 Key 之前 `/v1/*` 會拒絕服務**（fail-closed）。這是刻意的：伺服器端形態
> 綁 `0.0.0.0`，沒配 Key 就把上游額度對全網開放是不可接受的。

### 無人值守部署

管理員和閘道 Key 都能用環境變數預置，跳過瀏覽器裡的手動步驟：

| 變數 | 說明 |
| --- | --- |
| `AGENT2API_ADMIN_USER` + `AGENT2API_ADMIN_PASSWORD` | 預置管理員（密碼明文填，啟動時自動轉 bcrypt）；也可用 `AGENT2API_ADMIN_PASSWORD_HASH` 直接給雜湊 |
| `AGENT2API_PROXY_API_KEY` | 預置一把閘道 Key，跳過「面板裡建第一把」 |
| `AGENT2API_PROXY_HOME` | 資料目錄 |
| `AGENT2API_HOST` / `AGENT2API_PROXY_PORT` | 監聽位址（預設 `0.0.0.0`）/ 連接埠（預設 `3065`） |
| `AGENT2API_PANEL_PORT` | 面板分連接埠：管理面單獨監聽該連接埠，主連接埠只留 `/v1/*` |
| `AGENT2API_ALLOW_NO_KEY` | 設 `1` 關閉 fail-closed（僅純內網） |
| `AGENT2API_LOG_FILE` | 日誌檔路徑，預設 `{資料目錄}/logs/server.log` |
| `AGENT2API_VERBOSE` | 設 `1` 開啟 debug 日誌 |

Linux 寫在 `/etc/agent2api/agent2api.env`；Windows 改服務登錄檔的
`HKLM\SYSTEM\CurrentControlSet\Services\Agent2API\Environment`（`install-service.ps1`
已經寫好基礎項，重跑腳本會覆蓋）。

> **帳號無法預置**：帳號需要掃碼 / 驗證碼 / 貼上憑證，這一步由上游協定決定，
> 繞不過去。批量部署可以把已經配好的**資料目錄整個拷過去**（SQLite 庫 +
> `account.key` 在同一個目錄裡），帳號隨之遷移。

## Docker

```bash
docker run -d --name agent2api --restart unless-stopped \
  -p 3065:3065 -v ./data:/data \
  aimodcc/agent2api:latest
```

瀏覽器開啟 `http://<主機>:3065`，首次進入會引導**註冊管理員帳號**；登入後在「閘道 Key」頁建立一把 API Key 給客戶端用 —— `http://<主機>:3065/v1` 即 OpenAI 相容端點（未建立 Key 前拒絕轉發）。所有狀態（SQLite 資料庫 / 設定 / 日誌）都落在 `./data` 一個卷裡。

compose 使用者（amd64 / arm64 都有映像）：

```yaml
services:
  agent2api:
    image: aimodcc/agent2api:latest
    container_name: agent2api
    restart: unless-stopped
    ports:
      - "3065:3065"
    volumes:
      - ./data:/data
```

環境變數（都可選）：

| 變數 | 說明 |
| --- | --- |
| `AGENT2API_ADMIN_USER` + `AGENT2API_ADMIN_PASSWORD` | 預置管理員帳號密碼（密碼明文填，啟動時自動轉雜湊）；不填走面板註冊 |
| `AGENT2API_PANEL_PORT` | 面板分連接埠：設定後面板（介面 + `/api/*`）單獨監聽該連接埠，公網只映射主連接埠即可把管理面留在內網（面板連接埠綁定迴環位址，寫 `127.0.0.1:3066:3066`） |
| `AGENT2API_HOST` / `AGENT2API_PROXY_PORT` | 監聽位址（預設 `0.0.0.0`）/ 連接埠（預設 `3065`） |
| `AGENT2API_ALLOW_NO_KEY` | 設 `1` 關閉 fail-closed（未設 Key 也放行 `/v1`，僅限純內網） |
| `AGENT2API_CAPTCHA_ENABLED` | 登入頁人機驗證元件：預設 `1` 開啟，`0` 關閉 |
| `AGENT2API_SELF_UPDATE` | 容器內的更新入口：預設 `1` 開啟（面板可「下載並更新」）；設 `0` 關閉，只走手動升級 |
| `WORKBUDDY_UPDATE_REPO` + `WORKBUDDY_UPDATE_API_BASE` | 進階：把「檢查更新 / 下載更新」換成自建源。前者換儲存庫（預設跟上游 `Agent2API/agent2api`，fork 後可改），後者換 API 根（預設 `https://api.github.com`，需返回與 GitHub 同形的 release JSON）。配了自建源後，下載白名單只額外放行**該源自己的網域**（按源的 scheme：配 `http://` 就允許明文下載，僅建議本地演練 / 內網映像用） |

從原始碼建置：複製本專案後 `docker compose up -d --build`（映像裡只有閘道與面板，不含 Rust 工具鏈）。

**網頁面板的功能差異**（源於「沒有本機桌面客戶端」）：網頁登入（WorkBuddy / Qoder / Cline）、手機驗證碼、貼上憑證完全可用；需要本機回呼的網頁登入（AutoClaw / CatPaw / Accio / CodeArts / Trae）與「匯入本機登入狀態」不可用，請改用貼上憑證。桌面端開著「同時開放網頁管理面板」時，用瀏覽器存取該面板也是這一套差異 —— 同一份介面、同一套判定。

### 容器內更新（選用）

面板的「軟體更新」頁在容器裡能直接「下載並更新」：下載官方 Release 裡的 `agent2api-server-linux-<arch>.tar.gz`（校驗 `.sha256`）→ 替換執行檔與 `ui/` → 行程優雅結束、由 `restart` 策略拉起同一個容器。由 `AGENT2API_SELF_UPDATE` 控制（Dockerfile 預設 `1`），設為 `0` 則只走手動升級。

同一個 `agent2api-server-linux-<arch>.tar.gz` 也是 [Linux 伺服器端](#linuxsystemd) 的安裝包：包內的 `agent2api-server` 與 `ui/` 是這條更新鏈按名取得的項目，另外還帶著 `install.sh` / `agent2api.service` / `agent2api.env.example`。容器更新鏈只認前兩個項目（其餘讀完即丟，見 `selfupdate::extract_bundle`），所以一份產物同時服務兩種用途 —— 容器裡解包替換，裸機上解包跑 `install.sh`。

代價要清楚：更新後容器標籤與**實際執行的版本會漂移**（`docker inspect` 顯示的仍是舊 tag），執行 `docker compose pull && up -d` 重建容器會回到映像裡的版本。想固定版本就別開這個入口。

## 驗證與客戶端接入

閘道起來後，用 curl 確認連線性（`model` 填 `GET /v1/models` 裡任一實際存在的名字）：

```bash
curl http://127.0.0.1:3065/health
curl http://127.0.0.1:3065/v1/models

curl http://127.0.0.1:3065/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"deepseek-v4.1-flash","messages":[{"role":"user","content":"你好"}],"stream":true}'
```

客戶端接入範例（Python SDK）：

```python
from openai import OpenAI

client = OpenAI(base_url="http://127.0.0.1:3065/v1", api_key="sk-local")
resp = client.chat.completions.create(
    model="deepseek-v4.1-flash",     # 清單裡有哪家就是哪家，見 GET /v1/models
    messages=[{"role": "user", "content": "你好"}],
)
print(resp.choices[0].message.content)
```

**瀏覽器裡的頁面**（自建 Web UI 等）用 `fetch` 直連會被跨來源預檢拒絕——閘道預設不回應 CORS，預檢落到 API Key 校驗上得 401。兩種解法：① 設定頁「安全 → 閘道跨來源存取」開啟（來源 `*`；此時任何網頁都能借本機閘道打上游，建議同時設定「閘道 Key」）；② 頁面與閘道同來源——本地靜態服務託管頁面並把 `/v1` 反向代理到 `127.0.0.1:3065`。

## 區域網路存取

預設只監聽 `127.0.0.1`。開啟「設定 → 一般 → 區域網路存取」後改聽所有網路介面卡，區域網路裝置把 API 位址指向本機 IP 即可共用（介面會給出完整位址）。出於安全，開啟前必須註冊面板管理員：管理介面要求管理員工作階段或閘道 Key，無可用 Key 時轉發介面同樣拒絕服務（開啟流程會自動補一把「預設」Key）；也可選擇同時開放網頁面板（預設關閉）。變更重新啟動後生效。

桌面端（Windows / macOS）安裝包裡就帶著網頁面板那一份 `ui/`，開啟「同時開放網頁管理面板」後區域網路裝置用瀏覽器存取同一網址即是管理介面 —— 與 Docker 版是同一份介面；**桌面視窗自己不走網頁登入**（沒有登入頁），它以本機第一把閘道 Key 作為憑證，開啟這個開關不需要你額外登入桌面端。網頁登入頁只在瀏覽器存取面板時出現。
