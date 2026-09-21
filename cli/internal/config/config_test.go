package config

import (
	"path/filepath"
	"testing"
)

func TestConfigLoadAndSave(t *testing.T) {
	tempHome := t.TempDir()
	t.Setenv("HOME", tempHome)

	// 1. Initial load when file does not exist
	cfg, err := Load()
	if err != nil {
		t.Fatalf("expected nil error on missing config, got %v", err)
	}
	if len(cfg) != 0 {
		t.Fatalf("expected empty map, got %v", cfg)
	}

	// 2. Save new key-values
	cfg["test_key"] = "test_value"
	cfg["nested"] = map[string]any{"num": 123}
	if err := Save(cfg); err != nil {
		t.Fatalf("failed to save config: %v", err)
	}

	// 3. Load saved configuration
	loaded, err := Load()
	if err != nil {
		t.Fatalf("failed to load saved config: %v", err)
	}
	if loaded["test_key"] != "test_value" {
		t.Errorf("expected test_value, got %v", loaded["test_key"])
	}

	// 4. Test get and set helpers
	if err := set("custom_setting", "enabled"); err != nil {
		t.Fatalf("set failed: %v", err)
	}
	val, ok := get("custom_setting")
	if !ok || val != "enabled" {
		t.Errorf("expected custom_setting=enabled, got val=%v, ok=%v", val, ok)
	}

	// 5. Test deleteKey
	if err := deleteKey("custom_setting"); err != nil {
		t.Fatalf("deleteKey failed: %v", err)
	}
	_, ok = get("custom_setting")
	if ok {
		t.Errorf("expected custom_setting to be deleted")
	}

	// 6. Test deleteKey for non-existent key returns errKeyNotFound
	err = deleteKey("non_existent_key")
	if err == nil {
		t.Errorf("expected error deleting non-existent key, got nil")
	}
}

func TestDefaultTunnel(t *testing.T) {
	tempHome := t.TempDir()
	t.Setenv("HOME", tempHome)

	// Initial load should fail because no default tunnel is set
	_, err := LoadDefaultTunnel()
	if err == nil {
		t.Fatalf("expected error when no default tunnel is set, got nil")
	}

	// Store default tunnel
	if err := StoreDefaultTunnel("tunnel-abc-123"); err != nil {
		t.Fatalf("failed to store default tunnel: %v", err)
	}

	// Load should succeed now
	id, err := LoadDefaultTunnel()
	if err != nil {
		t.Fatalf("expected default tunnel id, got error: %v", err)
	}
	if id != "tunnel-abc-123" {
		t.Errorf("expected tunnel-abc-123, got %s", id)
	}

	// Delete default tunnel
	if err := DeleteDefaultTunnel(); err != nil {
		t.Fatalf("failed to delete default tunnel: %v", err)
	}

	// Load should fail again
	_, err = LoadDefaultTunnel()
	if err == nil {
		t.Errorf("expected error after default tunnel deleted, got nil")
	}

	// Deleting non-existent default tunnel should not error
	if err := DeleteDefaultTunnel(); err != nil {
		t.Errorf("expected nil error deleting non-existent default tunnel, got %v", err)
	}
}

func TestConfigPath(t *testing.T) {
	tempHome := t.TempDir()
	t.Setenv("HOME", tempHome)

	path, err := configPath()
	if err != nil {
		t.Fatalf("expected nil error, got %v", err)
	}
	expectedPath := filepath.Join(tempHome, ".huawei", "devbridge", "config.yaml")
	if path != expectedPath {
		t.Errorf("expected %s, got %s", expectedPath, path)
	}
}
