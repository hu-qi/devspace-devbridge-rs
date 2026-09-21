package cmd

import (
	"strings"
	"testing"

	"huawei.com/devbridge/internal/api"
)

func TestValidateTunnelID(t *testing.T) {
	tests := []struct {
		name      string
		id        string
		expectErr bool
	}{
		{"valid alphanumeric", "tunnel123", false},
		{"valid hyphen and underscore", "my-tunnel_01", false},
		{"valid single char", "a", false},
		{"valid max 64 chars", strings.Repeat("a", 64), false},
		{"empty id", "", true},
		{"too long (>64 chars)", strings.Repeat("a", 65), true},
		{"invalid characters - space", "tunnel 1", true},
		{"invalid characters - slash", "tunnel/1", true},
		{"invalid characters - dot", "tunnel.1", true},
		{"invalid characters - chinese", "隧道", true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := validateTunnelID(tt.id)
			if tt.expectErr && err == nil {
				t.Errorf("expected error for ID %q, got nil", tt.id)
			}
			if !tt.expectErr && err != nil {
				t.Errorf("unexpected error for ID %q: %v", tt.id, err)
			}
		})
	}
}

func TestValidatePorts(t *testing.T) {
	tests := []struct {
		name      string
		ports     []int
		expectErr bool
	}{
		{"valid single port", []int{8080}, false},
		{"valid multiple ports", []int{80, 443, 8080}, false},
		{"valid all-ports sentinel", []int{-1}, false},
		{"empty ports", []int{}, true},
		{"zero port", []int{0}, true},
		{"negative port other than -1", []int{-2}, true},
		{"port above 65535", []int{65536}, true},
		{"mix of valid and invalid", []int{80, 70000}, true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := validatePorts(tt.ports)
			if tt.expectErr && err == nil {
				t.Errorf("expected error for ports %v, got nil", tt.ports)
			}
			if !tt.expectErr && err != nil {
				t.Errorf("unexpected error for ports %v: %v", tt.ports, err)
			}
		})
	}
}

func TestPortResultsToInt(t *testing.T) {
	results := []api.ListPortsResult{
		{Port: 8080},
		{Port: 3000},
		{Port: 9000},
	}
	ports := portResultsToInt(results)
	if len(ports) != 3 {
		t.Fatalf("expected 3 ports, got %d", len(ports))
	}
	if ports[0] != 8080 || ports[1] != 3000 || ports[2] != 9000 {
		t.Errorf("unexpected ports result: %v", ports)
	}

	empty := portResultsToInt(nil)
	if len(empty) != 0 {
		t.Errorf("expected empty slice, got %v", empty)
	}
}
