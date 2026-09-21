package i18n

import (
	"testing"
)

func TestDetectLang(t *testing.T) {
	tests := []struct {
		name     string
		envVal   string
		expected Lang
	}{
		{"empty defaults to EN", "", EN},
		{"zh lower", "zh", ZH},
		{"zh_CN prefix", "zh_CN.UTF-8", ZH},
		{"zh-CN hyphen", "zh-CN", ZH},
		{"ZH uppercase", "ZH", ZH},
		{"en explicitly", "en", EN},
		{"en_US explicitly", "en_US.UTF-8", EN},
		{"other language fallback", "fr", EN},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Setenv("DEVBRIDGE_LANG", tt.envVal)
			got := detectLang()
			if got != tt.expected {
				t.Errorf("detectLang() with env=%q: expected %v, got %v", tt.envVal, tt.expected, got)
			}
		})
	}
}

func TestT(t *testing.T) {
	msg := Message{
		ZH: "你好世界",
		EN: "Hello World",
	}

	// Switch to ZH
	currentLang = ZH
	if got := T(msg); got != "你好世界" {
		t.Errorf("expected '你好世界', got %q", got)
	}

	// Switch to EN
	currentLang = EN
	if got := T(msg); got != "Hello World" {
		t.Errorf("expected 'Hello World', got %q", got)
	}
}
