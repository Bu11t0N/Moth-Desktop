// Independent Node/OpenSSL fixture for the Android 1.0.3 Backup.kt wire format.
// Synthetic data only. Run from the repository root; no networking or OS changes.
import { pbkdf2Sync, createCipheriv } from "node:crypto";
import { writeFileSync, mkdirSync } from "node:fs";
const password = "synthetic-android-password";
const salt = Buffer.from("000102030405060708090a0b0c0d0e0f", "hex");
const iv = Buffer.from("101112131415161718191a1b", "hex");
const payload = {
  settings: { hwid: "synthetic-phone-id", favorites: ["one"], routingRules: [
    {type: "domain_suffix", value: "example.com", action: "direct", enabled: false, name: "First"},
  ] },
  subscriptions: { selected: "one", sources: [
    {id: "local", url: "moth:local", profiles: [{id: "one", name: "Synthetic", link: "trojan://synthetic@example.com:443", outbound: "{}"}]},
  ] },
};
const cipher = createCipheriv("aes-256-gcm", pbkdf2Sync(password, salt, 210000, 32, "sha256"), iv);
cipher.setAAD(Buffer.from("moth-backup/1"));
const data = Buffer.concat([cipher.update(JSON.stringify(payload), "utf8"), cipher.final(), cipher.getAuthTag()]);
mkdirSync("crates/moth-core/tests/fixtures", {recursive: true});
writeFileSync("crates/moth-core/tests/fixtures/android-backup-v1.json", JSON.stringify({
  format: "moth-backup", version: 1, encrypted: true, kdf: "PBKDF2WithHmacSHA256", iterations: 210000,
  salt: salt.toString("base64"), iv: iv.toString("base64"), data: data.toString("base64"),
}, null, 2) + "\n");
