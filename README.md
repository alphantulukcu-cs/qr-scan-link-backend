# scan-link-backend

Tek kullanımlık çek tarama linki sistemi için Rust + Axum backend.

Bu servis, şube çalışanının müşteri için link üretmesini, linkin müşteri tarafında doğrulanmasını, çek görselleri + QR metadata gönderimini ve şube ekranında sonuçların listelenmesini yönetir.

## Bileşenler

- `branch-test-ui`: Şube çalışanı ekranı (link üretir, gönderimleri izler)
- `qr-scanner-ui`: Müşteri ekranı (`/capture/:inviteToken`)
- `scan-link-backend`: Invite/claim/submit REST API
- `postgres`: Invite, çek ve metadata kalıcılığı
- `nginx` (qr-scanner-ui önünde): HTTPS ve `/api` reverse proxy

## Uçtan Uca Akış

1. Şube çalışanı `POST /api/branch/invites` çağırır.
2. Backend tek kullanımlık davet tokenı üretir, hash'ini DB'ye kaydeder, mail linkini yollar.
3. Müşteri maildeki `https://.../capture/{inviteToken}` linkini açar.
4. `qr-scanner-ui` `GET /api/public/invites/{inviteToken}/claim` çağırır.
5. Backend linkin geçerliliğini kontrol eder ve session token döner.
6. Müşteri çekleri tarar, toplu fotoğraf çeker, `Çekleri Gönder` ile submit eder.
7. `POST /api/public/sessions/{invite_id}/submit` ile çek görselleri + QR metadata + session metadata kaydolur.
8. Şube ekranı `GET /api/branch/invites` ve `GET /api/branch/invites/{invite_id}` ile sonucu görür.

## Tek Kullanım ve Güvenlik Modeli

### 1) Link token güvenliği

- Dışarıya verilen token düz metin olarak sadece linkte bulunur.
- DB'de tokenın kendisi değil `SHA-256` hash'i tutulur (`one_time_token_hash`).
- Claim sırasında gelen token hash'lenip DB karşılaştırması yapılır.

### 2) Linkin kullanım politikası (güncel davranış)

- Link, `expires_at` dolana kadar tekrar açılabilir.
- Link `submitted` olduktan sonra tekrar kullanılamaz.
- Link süresi dolduğunda `expired` durumuna alınır.

### 3) Tek sefer gönderim garantisi

- Submit çağrısında `x-session-token` zorunludur.
- Session token hash'i DB'deki `session_token_hash` ile eşleşmek zorundadır.
- İlk başarılı submit sonrası:
  - `status = submitted`
  - `session_token_hash = NULL`
- Aynı invite için ikinci submit `409 conflict` döner.

### 4) UI erişim kısıtı

- `qr-scanner-ui` yalnızca `/capture/:inviteToken` rotasını açar.
- Nginx tarafında `/`, `/home`, kredi rotaları `403` döner.
- Böylece kullanıcı link olmadan akışa giremez.

### 5) Ağ ve payload korumaları

- CORS allow-list ile sınırlandırılır (`CORS_ALLOWED_ORIGINS`).
- Nginx private network allow-list ile çalışır.
- Büyük payload için limitler:
  - Nginx: `client_max_body_size 30m`
  - Axum: `DefaultBodyLimit::max(30 * 1024 * 1024)`

## API

- `GET /api/health`  
  Sağlık kontrolü.

- `POST /api/branch/invites`  
  Yeni davet linki üretir ve mail göndermeyi dener.

- `GET /api/branch/invites`  
  Son davetleri özet listeler.

- `GET /api/branch/invites/{invite_id}`  
  Davet detayını, çekleri, görselleri ve metadata'yı döner.

- `GET /api/public/invites/{invite_token}/claim`  
  Linki doğrular, aktif session token döner.

- `POST /api/public/sessions/{invite_id}/submit`  
  Çek görselleri + QR metadata + session metadata gönderimini finalize eder.

## Veritabanı Özeti

### `scan_invites`

- `invite_id` (UUID, PK)
- `one_time_token_hash` (TEXT, unique)
- `session_token_hash` (TEXT, nullable)
- `customer_national_id`, `customer_email`
- `status` (`pending|claimed|submitted|expired`)
- `claim_count`, `created_at`, `expires_at`, `claimed_at`, `submitted_at`
- `batch_image_data_url` (TEXT)
- `session_metadata` (JSONB)

### `scan_checks`

- `id` (BIGSERIAL, PK)
- `invite_id` (FK -> scan_invites)
- `sequence_no`, `qr_value`
- `image_data_url` (TEXT)
- `captured_at`, `created_at`
- `metadata` (JSONB)

Not: Görseller şu an DB'de `data:image/...;base64,...` formatında saklanır.

## Kurulum

### 1) PostgreSQL başlat

```bash
docker compose up -d postgres
```

### 2) `.env` dosyasını oluştur

Örnek:

```env
APP_ADDR=0.0.0.0:8095
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5432/scan_link_backend
INVITE_BASE_URL=https://10.10.10.246:8443/capture
INVITE_TTL_MINUTES=120
CORS_ALLOWED_ORIGINS=http://127.0.0.1:5173,http://localhost:5173,https://10.10.10.246:8443
SERVICE_NAME=scan-link-backend

SMTP_HOST=smtp.gmail.com
SMTP_PORT=587
SMTP_USERNAME=you@example.com
SMTP_PASSWORD=app_password
SMTP_FROM_ADDRESS=you@example.com
SMTP_FROM_NAME=Sekerbank Istihbarat Sistemi
# SMTP_LOGO_URL=https://ornek.domain/logo.svg
```

Not: `SMTP_LOGO_URL` opsiyoneldir. Tanımlanmazsa logo, backend içindeki
`Şekerbank_logo.svg` dosyasından doğrudan mail HTML gövdesine eklenir.

### 3) Servisi çalıştır

```bash
cargo run
```

Varsayılan adres: `http://127.0.0.1:8095`

## Operasyonel Notlar

- `Request Entity Too Large` alırsan backend ve nginx yeniden başlatılmalı (yeni limitlerin etkili olması için).
- `claim` aşamasında `not_found/expired/submitted` dönmesi beklenen güvenlik davranışıdır.
- SMTP alanları boşsa invite kaydı oluşur, sadece mail gönderimi atlanır.

## Registry Tabanlı Deployment

Bu repo içinde local build yerine registry'den image pull edecek deployment akışı hazırdır.

### 1) Image'ları build ve push et

```bash
docker login -u alphantulukcucs
./scripts/build-and-push-images.sh alphantulukcucs/scan-link v1.0.0
```

Bu komut tek private repo içine 3 farklı tag push eder:
- `alphantulukcucs/scan-link:backend-v1.0.0`
- `alphantulukcucs/scan-link:branch-ui-v1.0.0`
- `alphantulukcucs/scan-link:qr-ui-v1.0.0`

Not: `PUSH_LATEST=true` verirsen ek olarak `backend-latest`, `branch-ui-latest`, `qr-ui-latest` tag'leri de push edilir.

### 2) Private makinede env dosyalarını hazırla

```bash
cp .env.registry.example .env.registry
```

- `.env.registry` içine push edilen image tag'lerini yaz.
- Uygulama secret/config değerleri için `.env` dosyasını ayrıca hazırla.

### 3) Registry'den pull edip ayağa kaldır

```bash
docker login -u alphantulukcucs
docker compose --env-file .env.registry -f docker-compose.registry.yml pull
docker compose --env-file .env.registry -f docker-compose.registry.yml up -d
```
