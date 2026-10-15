import { describe, expect, it, vi } from "vitest";
import {
  accountAvatarMap,
  AvatarCache,
  avatarSources,
  contrastText,
  DARK_TEXT,
  githubNoreplyAvatar,
  gravatarUrl,
  initials,
  initialsBadge,
  LIGHT_TEXT,
  md5,
} from "./avatars";

describe("md5", () => {
  it("matches the RFC 1321 test vectors", () => {
    expect(md5("")).toBe("d41d8cd98f00b204e9800998ecf8427e");
    expect(md5("a")).toBe("0cc175b9c0f1b6a831c399e269772661");
    expect(md5("abc")).toBe("900150983cd24fb0d6963f7d28e17f72");
    expect(md5("message digest")).toBe("f96b697d7cb7938d525a2f31aaf161d0");
    expect(md5("abcdefghijklmnopqrstuvwxyz")).toBe("c3fcd3d76192e4007dfb496cca67e13b");
    expect(md5("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789")).toBe(
      "d174ab98d277d9f5a5611c2c9f419d9f",
    );
    expect(md5("1234567890".repeat(8))).toBe("57edf4a22be3c955ac49da2e2107b67a");
  });

  it("hashes UTF-8 bytes and the 55/56/64-byte padding edges", () => {
    expect(md5("The quick brown fox jumps over the lazy dog")).toBe(
      "9e107d9d372bb6826bd81d3542a419d6",
    );
    expect(md5("é")).toBe("66ddcd97cfdeabb2f6fb8a999b4bc76f");
    expect(md5("a".repeat(55))).toBe("ef1772b6dff9a122358552954ad0df65");
    expect(md5("a".repeat(56))).toBe("3b0c8ac703f828b04c6c197006d17218");
    expect(md5("a".repeat(64))).toBe("014842d480b571495a4a0363793f7367");
  });
});

describe("avatar sources", () => {
  it("maps GitHub noreply addresses to GitHub avatars", () => {
    expect(githubNoreplyAvatar("12345+octo-cat@users.noreply.github.com")).toBe(
      "https://avatars.githubusercontent.com/u/12345?s=64&v=4",
    );
    expect(githubNoreplyAvatar(" Octo@users.noreply.github.com ")).toBe(
      "https://github.com/octo.png?size=64",
    );
    expect(githubNoreplyAvatar("octo@example.com")).toBeNull();
  });

  it("hashes the trimmed, lowercased email for Gravatar", () => {
    expect(gravatarUrl(" MyEmailAddress@example.com ")).toBe(
      "https://www.gravatar.com/avatar/0bc83cb571cd1c50ba6f3e8a78ef1346?s=64&d=404",
    );
  });

  it("orders noreply, account avatar, then Gravatar", () => {
    const accounts = accountAvatarMap(
      [
        { id: "a1", avatarUrl: "https://avatars.example/a1.png" },
        { id: "a2", avatarUrl: null },
      ],
      [
        { accountId: "a1", email: "Dev@Acme.vn" },
        { accountId: "a2", email: "x@y.z" },
        { accountId: "a1", email: "1+dev@users.noreply.github.com" },
      ],
    );
    expect([...accounts.keys()]).toEqual(["dev@acme.vn", "1+dev@users.noreply.github.com"]);
    expect(avatarSources("dev@acme.vn", accounts)).toEqual([
      "https://avatars.example/a1.png",
      gravatarUrl("dev@acme.vn"),
    ]);
    expect(avatarSources("1+dev@users.noreply.github.com", accounts)).toEqual([
      "https://avatars.githubusercontent.com/u/1?s=64&v=4",
      "https://avatars.example/a1.png",
      gravatarUrl("1+dev@users.noreply.github.com"),
    ]);
    expect(avatarSources("someone@else.org", accounts)).toEqual([gravatarUrl("someone@else.org")]);
    expect(avatarSources("not an email", accounts)).toEqual([]);
  });
});

describe("initials and contrast", () => {
  it("takes the first letters of the first and last word", () => {
    expect(initials("Nguyễn Văn Hoàng", "h@acme.vn")).toBe("NH");
    expect(initials("linus", "l@kernel.org")).toBe("L");
    expect(initials("  ", "zed@x.y")).toBe("Z");
    expect(initials("", "")).toBe("?");
    expect(initials("ömer (bot) Ünal", "")).toBe("ÖÜ");
  });

  it("picks dark text on light colors and white on dark ones", () => {
    expect(contrastText("#ffd43b")).toBe(DARK_TEXT);
    expect(contrastText("#69db7c")).toBe(DARK_TEXT);
    expect(contrastText("#1971c2")).toBe(LIGHT_TEXT);
    expect(contrastText("#c92a2a")).toBe(LIGHT_TEXT);
    expect(contrastText("rgb(0 0 0)")).toBe(LIGHT_TEXT);
  });

  it("colors an author stably from the lane palette", () => {
    const lanes = ["#ffd43b", "#1971c2", "#c92a2a"];
    const a = initialsBadge("Ann Lee", "ann@x.y", lanes);
    expect(initialsBadge("Ann Lee", "ann@x.y", lanes)).toBe(a);
    expect(lanes).toContain(a.color);
    expect(a.text).toBe("AL");
    expect(a.fg).toBe(contrastText(a.color));
  });
});

class FakeImage {
  src = "";
  decoding = "";
  referrerPolicy = "";
  naturalWidth = 64;
  onload: (() => void) | null = null;
  onerror: (() => void) | null = null;
}

function setup(maxConcurrent = 6) {
  const images: FakeImage[] = [];
  const onLoad = vi.fn();
  const cache = new AvatarCache({
    onLoad,
    maxConcurrent,
    createImage: () => {
      const img = new FakeImage();
      images.push(img);
      return img as unknown as HTMLImageElement;
    },
  });
  return { cache, images, onLoad };
}

describe("AvatarCache", () => {
  it("loads once, then returns the image", () => {
    const { cache, images, onLoad } = setup();
    expect(cache.get("a@x.y")).toBeNull();
    expect(cache.get("a@x.y")).toBeNull();
    expect(images).toHaveLength(1);
    expect(images[0]!.src).toBe(gravatarUrl("a@x.y"));
    images[0]!.onload!();
    expect(onLoad).toHaveBeenCalledTimes(1);
    expect(cache.get("a@x.y")).toBe(images[0]);
    expect(images).toHaveLength(1);
  });

  it("falls back to the next source and remembers failures", () => {
    const { cache, images } = setup();
    const email = "7+bob@users.noreply.github.com";
    cache.get(email);
    expect(images[0]!.src).toContain("avatars.githubusercontent.com/u/7");
    images[0]!.onerror!();
    expect(images[1]!.src).toBe(gravatarUrl(email));
    images[1]!.onerror!();
    expect(images).toHaveLength(2);
    // Every source failed: initials from now on, no new request.
    expect(cache.get(email)).toBeNull();
    expect(images).toHaveLength(2);
  });

  it("caps concurrent loads and serves the newest request first", () => {
    const { cache, images } = setup(2);
    for (const e of ["1@x.y", "2@x.y", "3@x.y", "4@x.y"]) cache.get(e);
    expect(images).toHaveLength(2);
    images[0]!.onload!();
    expect(images).toHaveLength(3);
    expect(images[2]!.src).toBe(gravatarUrl("4@x.y"));
  });

  it("loads nothing while disabled", () => {
    const { cache, images } = setup();
    cache.setEnabled(false);
    expect(cache.get("a@x.y")).toBeNull();
    expect(images).toHaveLength(0);
    cache.setEnabled(true);
    cache.get("a@x.y");
    expect(images).toHaveLength(1);
  });

  it("re-resolves authors when the account avatars change", () => {
    const { cache, images } = setup();
    cache.get("dev@acme.vn");
    cache.setAccountAvatars(new Map([["dev@acme.vn", "https://avatars.example/a.png"]]));
    cache.get("dev@acme.vn");
    expect(images.at(-1)!.src).toBe("https://avatars.example/a.png");
  });
});
