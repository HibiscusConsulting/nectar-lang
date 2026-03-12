class NectarLang < Formula
  desc "A compiled-to-WASM frontend language with built-in security, SEO, and mobile support"
  homepage "https://buildnectar.com"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/BlakeBurnette/nectar-lang/releases/latest/download/nectar-latest-aarch64-apple-darwin.tar.gz"
    else
      url "https://github.com/BlakeBurnette/nectar-lang/releases/latest/download/nectar-latest-x86_64-apple-darwin.tar.gz"
    end
  end

  on_linux do
    url "https://github.com/BlakeBurnette/nectar-lang/releases/latest/download/nectar-latest-x86_64-unknown-linux-gnu.tar.gz"
  end

  def install
    bin.install "nectar"
  end

  test do
    system "#{bin}/nectar", "--version"
  end
end
