package auth

import (
	"testing"
)

func TestMaskSecret(t *testing.T) {
	tests := []struct {
		input    string
		expected string
	}{
		{"", "****"},
		{"1234", "****"},
		{"12345678", "****"},
		{"123456789", "1234****6789"},
		{"my-very-long-secret-key-123456", "my-v****3456"},
	}

	for _, tt := range tests {
		got := maskSecret(tt.input)
		if got != tt.expected {
			t.Errorf("maskSecret(%q): expected %q, got %q", tt.input, tt.expected, got)
		}
	}
}

func TestSetOverrideAPIKey(t *testing.T) {
	// Clean up after test
	defer SetOverrideAPIKey("")

	SetOverrideAPIKey("test-api-key-999")
	cred := ReadValidAPIKey()
	if cred == nil {
		t.Fatalf("expected non-nil credential")
	}
	if cred.APIKey != "test-api-key-999" {
		t.Errorf("expected test-api-key-999, got %q", cred.APIKey)
	}
}

func TestIsValidAPIKey(t *testing.T) {
	if isValidAPIKey(nil) {
		t.Errorf("expected false for nil credential")
	}
	if isValidAPIKey(&Credential{APIKey: ""}) {
		t.Errorf("expected false for empty apiKey")
	}
	if !isValidAPIKey(&Credential{APIKey: "valid-key"}) {
		t.Errorf("expected true for non-empty apiKey")
	}
}
