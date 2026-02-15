# UI.md - User Interface Architecture

**Authority:** guidance (UI patterns and component architecture)
**Layer:** Guides
**Binding:** No
**Scope:** UI architecture patterns, component design, interaction models, and rendering strategies
**Non-goals:** specific framework implementations, visual design systems, or branding guidelines

This document defines architectural patterns for building user interfaces within Decapod-managed systems.

---

## 1. UI Architecture Philosophy

### 1.1 Intent-Driven UI

User interfaces in Decapod follow the same intent-first methodology as the backend:

```
User Intent → UI State → Component Tree → Render Output
```

The UI is a **projection of state**, not a source of truth. All mutations flow through the control plane.

### 1.2 Core Principles

1. **State at the Center**: UI components render state; they don't own it
2. **Unidirectional Flow**: User actions → Control plane → State update → Re-render
3. **Explicit Over Implicit**: Every interaction has a declared intent
4. **Proof in the UI**: Validation gates surface in the interface

---

## 2. UI Component Architecture

### 2.1 Component Layers

```
┌─────────────────────────────────────────┐
│  Presentation Layer (Views/Pages)       │
│  - Route-level components               │
│  - Layout containers                    │
└─────────────────────────────────────────┘
                    │
┌─────────────────────────────────────────┐
│  Container Layer (Smart Components)     │
│  - Connect to control plane             │
│  - Manage local UI state                │
│  - Handle user intent                   │
└─────────────────────────────────────────┘
                    │
┌─────────────────────────────────────────┐
│  Component Layer (Dumb Components)      │
│  - Pure render functions                │
│  - Props in, events out                 │
│  - No side effects                      │
└─────────────────────────────────────────┘
                    │
┌─────────────────────────────────────────┐
│  Primitive Layer (Design Tokens)        │
│  - Buttons, inputs, text                │
│  - Theme-aware                          │
│  - Accessibility first                  │
└─────────────────────────────────────────┘
```

### 2.2 Control Plane Integration

UI components interact with Decapod through a **Control Plane Adapter**:

```typescript
// Conceptual interface
interface ControlPlaneAdapter {
  // Read state from control plane
  query<T>(command: string, params?: object): Promise<T>;
  
  // Mutate state through control plane
  execute(command: string, params?: object): Promise<Result>;
  
  // Subscribe to state changes
  subscribe(event: string, callback: Handler): Subscription;
}
```

**Rule**: No component talks directly to the store. All access goes through the adapter.

---

## 3. State Management Patterns

### 3.1 UI State vs Domain State

| Type | Location | Examples | Mutated By |
|------|----------|----------|------------|
| **Domain State** | Control plane | TODOs, validation results, proofs | `decapod` commands |
| **UI State** | Component local | Modal open/close, form input, selected tab | User interactions |
| **URL State** | Browser | Current route, query params, filters | Navigation |

### 3.2 State Synchronization

```
User Action → UI Event → Intent Declaration → Control Plane → State Update → Re-render
```

**Example: Marking a TODO complete**

```typescript
// User clicks "Done" button
// UI component emits intent
const intent = {
  type: 'TODO_COMPLETE',
  payload: { todoId: 'R_XXXXXXXX' }
};

// Control plane adapter executes
await controlPlane.execute('todo done', { id: todoId });

// State updates, UI re-renders
```

---

## 4. Rendering Strategies

### 4.1 Server vs Client Rendering

**Server-Side Rendering (SSR)**:
- Initial page load
- SEO-critical content
- Control plane state snapshot at request time

**Client-Side Rendering (CSR)**:
- Post-load interactions
- Real-time updates
- Dynamic state changes

**Hybrid Approach**:
- SSR for initial state
- CSR for subsequent interactions
- Progressive enhancement

### 4.2 Real-Time Updates

For live UI updates:

```
Control Plane Event Stream → Adapter → Component Update
```

Options:
- **Polling**: Periodic `decapod validate` or specific queries
- **Server-Sent Events**: Push updates from control plane
- **WebSockets**: Bidirectional real-time (if needed)

---

## 5. UI Validation & Proof Gates

### 5.1 Validation in the UI

Validation results from `decapod validate` should surface in the UI:

```typescript
interface ValidationSummary {
  status: 'pass' | 'fail' | 'warning';
  totalChecks: number;
  passed: number;
  failed: number;
  gates: ValidationGate[];
}

interface ValidationGate {
  name: string;
  status: 'pass' | 'fail' | 'warning' | 'info';
  message: string;
  details?: object;
}
```

### 5.2 Proof Visualization

Display proof status visually:

- ✅ **Pass**: Green indicator, checkmark
- ❌ **Fail**: Red indicator, X mark, action required
- ⚠️ **Warning**: Yellow indicator, attention needed
- ℹ️ **Info**: Blue indicator, informational

---

## 6. Component Design Patterns

### 6.1 Intent Components

Components that capture user intent:

```typescript
// Intent capture pattern
interface IntentButtonProps {
  intent: string;           // e.g., "TODO_CREATE"
  payload?: object;         // Intent data
  validate?: boolean;       // Run validation first?
  onIntent?: (result) => void;  // Callback after execution
}
```

### 6.2 Proof-Aware Components

Components that display proof status:

```typescript
interface ProofBadgeProps {
  claimId: string;          // e.g., "claim.doc.real_requires_proof"
  status: 'verified' | 'unverified' | 'stale';
  lastVerified?: Date;
  proofSurface?: string;    // e.g., "decapod validate"
}
```

### 6.3 State Boundary Components

Components that enforce state boundaries:

```typescript
interface StoreBoundaryProps {
  store: 'user' | 'repo';   // Which store scope?
  children: ReactNode;
}

// Enforces: child components only access specified store
```

---

## 7. Accessibility (A11y)

### 7.1 Required Standards

- **WCAG 2.1 Level AA**: Minimum compliance target
- **Keyboard Navigation**: All interactions via keyboard
- **Screen Reader Support**: Semantic HTML, ARIA labels
- **Color Contrast**: 4.5:1 minimum for text

### 7.2 Semantic Structure

```html
<!-- Good: Semantic structure -->
<main>
  <nav aria-label="Primary">...</nav>
  <article>
    <header>...</header>
    <section aria-labelledby="validation-heading">
      <h2 id="validation-heading">Validation Results</h2>
      ...
    </section>
  </article>
</main>

<!-- Bad: Div soup -->
<div class="app">
  <div class="nav">...</div>
  <div class="content">
    <div class="header">...</div>
    <div class="section">...</div>
  </div>
</div>
```

---

## 8. Error Handling

### 8.1 UI Error Boundaries

Catch and display errors gracefully:

```typescript
interface ErrorState {
  type: 'validation' | 'network' | 'control_plane' | 'unknown';
  message: string;
  recoverable: boolean;
  suggestedAction?: string;
}
```

### 8.2 Control Plane Errors

When `decapod` commands fail:

1. Display error message clearly
2. Log to console for debugging
3. Provide retry action if recoverable
4. Route to emergency protocol if critical

---

## 9. Performance Patterns

### 9.1 Lazy Loading

Load components on demand:

```typescript
// Route-level lazy loading
const ValidationDashboard = lazy(() => import('./ValidationDashboard'));

// Component-level lazy loading
const HeavyChart = lazy(() => import('./HeavyChart'));
```

### 9.2 State Memoization

Memoize expensive computations:

```typescript
// Memoize validation results
const validationSummary = useMemo(() => {
  return computeSummary(validationResults);
}, [validationResults]);

// Memoize component rendering
const TodoList = memo(({ todos }) => {
  return <ul>{todos.map(renderTodo)}</ul>;
});
```

### 9.3 Debounced Interactions

Debounce rapid user actions:

```typescript
// Debounce search input
const debouncedSearch = useDebounce(searchQuery, 300);

// Debounce control plane calls
const debouncedValidate = useDebounce(runValidation, 1000);
```

---

## 10. Testing Strategy

### 10.1 Component Testing

```typescript
// Test component rendering
describe('ValidationBadge', () => {
  it('renders success state', () => {
    render(<ValidationBadge status="pass" />);
    expect(screen.getByText('✅ PASS')).toBeInTheDocument();
  });
  
  it('calls control plane on click', async () => {
    const mockExecute = jest.fn();
    render(<TodoCompleteButton todoId="123" execute={mockExecute} />);
    
    await userEvent.click(screen.getByRole('button'));
    expect(mockExecute).toHaveBeenCalledWith('todo done', { id: '123' });
  });
});
```

### 10.2 Integration Testing

```typescript
// Test control plane integration
describe('Control Plane Adapter', () => {
  it('fetches TODO list', async () => {
    const todos = await adapter.query('todo list');
    expect(todos).toHaveLength(3);
  });
  
  it('executes TODO completion', async () => {
    const result = await adapter.execute('todo done', { id: '123' });
    expect(result.status).toBe('success');
  });
});
```

---

## 11. Security Considerations

### 11.1 XSS Prevention

- Sanitize all user input
- Use framework escaping (React's `{}`, Vue's `{{}}`)
- Avoid `dangerouslySetInnerHTML` / `v-html`

### 11.2 State Sanitization

Validate all control plane responses:

```typescript
// Validate response shape
const todoSchema = z.object({
  id: z.string(),
  title: z.string(),
  status: z.enum(['open', 'done', 'archived']),
  priority: z.enum(['high', 'medium', 'low'])
});

const validated = todoSchema.parse(response);
```

### 11.3 Secure Defaults

- No sensitive data in URLs
- No secrets in client-side code
- HTTPS only for control plane communication

---

## 12. Implementation Guidance

### 12.1 Framework-Agnostic Patterns

This document describes patterns that work with:
- React
- Vue
- Svelte
- Vanilla JS
- Any framework with component model

### 12.2 Technology Choices

Document framework-specific choices in project-level docs:
- State management library (if any)
- Component library
- Styling approach
- Build tooling

### 12.3 Migration Path

For existing UIs:

1. **Phase 1**: Add control plane adapter layer
2. **Phase 2**: Migrate state to control plane
3. **Phase 3**: Refactor components to new patterns
4. **Phase 4**: Add UI validation gates

---

## Links

### Core Router
- `core/DECAPOD.md` - **Router and navigation charter (START HERE)**

### Authority (Constitution Layer)
- `specs/INTENT.md` - **Methodology contract (READ FIRST)**
- `specs/SYSTEM.md` - System definition and authority doctrine
- `specs/SECURITY.md` - Security contract

### Registry (Core Indices)
- `core/PLUGINS.md` - Subsystem registry
- `core/INTERFACES.md` - Interface contracts index
- `core/METHODOLOGY.md` - Methodology guides index
- `core/GAPS.md` - Gap analysis methodology

### Contracts (Interfaces Layer)
- `interfaces/CONTROL_PLANE.md` - Sequencing patterns
- `interfaces/STORE_MODEL.md` - Store semantics
- `interfaces/CLAIMS.md` - Promises ledger
- `interfaces/TESTING.md` - Testing contract

### Practice (Methodology Layer - Related Documents)
- `methodology/SOUL.md` - Agent identity
- `methodology/ARCHITECTURE.md` - Architecture practice

### Architecture Patterns (Related Domain Docs)
- `architecture/FRONTEND.md` - Frontend architecture patterns
- `architecture/WEB.md` - Web architecture patterns
- `architecture/SECURITY.md` - Security architecture

### Operations (Plugins Layer)
- `plugins/TODO.md` - Work tracking
- `plugins/VERIFY.md` - Validation subsystem
- `plugins/EMERGENCY_PROTOCOL.md` - Emergency protocols
