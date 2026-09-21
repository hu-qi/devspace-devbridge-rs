package netutil

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

func TestPingURI_Success(t *testing.T) {
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte("pong"))
	}))
	defer ts.Close()

	res := PingURI(ts.URL, 2*time.Second)
	if res.Err != nil {
		t.Fatalf("expected nil error, got %v", res.Err)
	}
	if !strings.Contains(res.StatusText, "200") {
		t.Errorf("expected status text containing 200, got %q", res.StatusText)
	}
	if res.Latency <= 0 {
		t.Errorf("expected positive latency, got %v", res.Latency)
	}
}

func TestPingURI_ServerError(t *testing.T) {
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusInternalServerError)
	}))
	defer ts.Close()

	res := PingURI(ts.URL, 2*time.Second)
	if res.Err != nil {
		t.Fatalf("expected nil error on HTTP 500 status code, got %v", res.Err)
	}
	if !strings.Contains(res.StatusText, "500") {
		t.Errorf("expected status text containing 500, got %q", res.StatusText)
	}
}

func TestPingURI_Timeout(t *testing.T) {
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		time.Sleep(200 * time.Millisecond)
		w.WriteHeader(http.StatusOK)
	}))
	defer ts.Close()

	// Short timeout of 50ms should trigger timeout
	res := PingURI(ts.URL, 50*time.Millisecond)
	if res.Err == nil {
		t.Fatalf("expected timeout error, got nil")
	}
	if res.StatusText != "Gateway Timeout" {
		t.Errorf("expected 'Gateway Timeout', got %q", res.StatusText)
	}
}

func TestPingURI_InvalidURL(t *testing.T) {
	res := PingURI("http://127.0.0.1:0", 500*time.Millisecond)
	if res.Err == nil {
		t.Fatalf("expected dial error, got nil")
	}
	if res.StatusText != "Bad Gateway" {
		t.Errorf("expected 'Bad Gateway', got %q", res.StatusText)
	}
}
