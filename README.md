# RtMaga

Ứng dụng đọc truyện tranh nhẹ, gọn viết bằng **Rust** (**Tauri 2** + **Svelte 5** + **Tailwind CSS 4**), hỗ trợ **Windows x64** và **Android ARM64 (Snapdragon)**.

## Tính năng chính

- **Dán link bất kỳ để đọc ngay**: hỗ trợ dán trực tiếp link trang truyện hoặc link chương (ưu tiên adapter riêng cho TruyenQQ, kèm bộ nhận diện tự động cho các web truyện khác và fallback trình duyệt headless Edge/Chrome trên Windows).
- **Proxy ảnh + bộ nhớ đệm cục bộ**: tự động gắn `Referer` đúng để vượt chặn hotlink, kèm cache ảnh LRU trên đĩa.
- **Trình đọc cuộn dọc liên tục**: tự động tải trước và nối liền chương tiếp theo kiểu Webtoon (có thể bật/tắt), lưu vị trí cuộn chính xác, thanh công cụ tự ẩn, phím tắt (`←/→`, `Space`, `F`, `Esc`).
- **Trang chủ · Lịch sử · Cài đặt**: giao diện tối giản, hỗ trợ chế độ Sáng / Tối / Theo hệ thống, tự kiểm tra chương mới cho các truyện trong lịch sử và gửi thông báo.

## Tải bản cài đặt

Xem mục [Releases](https://github.com/DUXGVEB/Rt-Maga/releases) để tải file cài đặt sẵn cho **Windows x64** (`.exe` / `.msi`) và **Android ARM64 / Snapdragon** (`.apk`).

## Phát triển & tự build

Yêu cầu: Rust 1.85+, Node.js 20+.

```powershell
# Cài dependencies UI
npm install
npm --prefix ui install

# Chạy chế độ phát triển (Windows)
npm run dev

# Build bộ cài đặt Windows x64 (.exe / .msi)
npm run build

# Kiểm thử Rust & kiểm tra lints
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

## Kiến trúc

- `crates/mangacore`: thư viện lõi lấy dữ liệu, parse HTML thuần (test được độc lập bằng fixture), không phụ thuộc Tauri.
- `src-tauri`: backend Tauri 2 (SQLite WAL qua `sqlx`, custom protocol `img://` ký HMAC-SHA256, disk cache LRU, tác vụ nền kiểm tra chương mới).
- `ui`: frontend Svelte 5 + TypeScript + Tailwind CSS 4.
