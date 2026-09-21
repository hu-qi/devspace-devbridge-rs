package connect

import (
	"sync"
	"testing"
)

func TestListenerFactoryConcurrentCreation(t *testing.T) {
	const numGoroutines = 20
	factory := newListenerFactory(numGoroutines)

	var wg sync.WaitGroup
	wg.Add(numGoroutines)

	for i := 0; i < numGoroutines; i++ {
		go func(remotePort int) {
			defer wg.Done()
			// Port 0 triggers dynamic port allocation safely on loopback interface
			ln, err := factory.CreateTCPListener(remotePort, "127.0.0.1", 0, true)
			if err != nil {
				t.Errorf("failed to create tcp listener for port %d: %v", remotePort, err)
				return
			}
			if ln == nil {
				t.Errorf("expected listener for port %d, got nil", remotePort)
			}
		}(10000 + i)
	}

	wg.Wait()

	// Verify forwardings count and listener count
	factory.mu.Lock()
	forwardingsCount := len(factory.pendingForwardings)
	listenersCount := len(factory.listeners)
	overridesCount := len(factory.portOverrides)
	factory.mu.Unlock()

	if forwardingsCount != numGoroutines {
		t.Fatalf("expected %d pending forwardings, got %d", numGoroutines, forwardingsCount)
	}
	if listenersCount != numGoroutines {
		t.Fatalf("expected %d listeners, got %d", numGoroutines, listenersCount)
	}
	if overridesCount != numGoroutines {
		t.Fatalf("expected %d overrides, got %d", numGoroutines, overridesCount)
	}

	// Verify reset closes all listeners cleanly
	factory.reset()

	factory.mu.Lock()
	afterResetListeners := len(factory.listeners)
	afterResetForwardings := len(factory.pendingForwardings)
	factory.mu.Unlock()

	if afterResetListeners != 0 {
		t.Errorf("expected 0 listeners after reset, got %d", afterResetListeners)
	}
	if afterResetForwardings != 0 {
		t.Errorf("expected 0 pending forwardings after reset, got %d", afterResetForwardings)
	}
}
