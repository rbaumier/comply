# screen-reader-testing

---
name: screen-reader-testing
description: Test web applications with screen readers including VoiceOver, NVDA, and JAWS. Use when validating screen reader compatibility, debugging accessibility issues, or ensuring assistive technology support.
---

# Screen Reader Testing

Practical guide to testing web applications with screen readers for comprehensive accessibility validation.

## When to Use This Skill

- Validating screen reader compatibility
- Testing ARIA implementations
- Debugging assistive technology issues
- Verifying form accessibility
- Testing dynamic content announcements
- Ensuring navigation accessibility

## Core Concepts

### 1. Major Screen Readers

| Screen Reader | Platform  | Browser        | Usage |
| ------------- | --------- | -------------- | ----- |
| **VoiceOver** | macOS/iOS | Safari         | ~15%  |
| **NVDA**      | Windows   | Firefox/Chrome | ~31%  |
| **JAWS**      | Windows   | Chrome/IE      | ~40%  |
| **TalkBack**  | Android   | Chrome         | ~10%  |
| **Narrator**  | Windows   | Edge           | ~4%   |

### 2. Testing Priority

```
Minimum Coverage:
1. NVDA + Firefox (Windows)
2. VoiceOver + Safari (macOS)
3. VoiceOver + Safari (iOS)

Comprehensive Coverage:
+ JAWS + Chrome (Windows)
+ TalkBack + Chrome (Android)
+ Narrator + Edge (Windows)
```

### 3. Screen Reader Modes

| Mode               | Purpose                | When Used         |
| ------------------ | ---------------------- | ----------------- |
| **Browse/Virtual** | Read content           | Default reading   |
| **Focus/Forms**    | Interact with controls | Filling forms     |
| **Application**    | Custom widgets         | ARIA applications |

## VoiceOver (macOS)

### Setup

```
Enable: System Preferences → Accessibility → VoiceOver
Toggle: Cmd + F5
Quick Toggle: Triple-press Touch ID
```

### Essential Commands

```
Navigation:
VO = Ctrl + Option (VoiceOver modifier)

VO + Right Arrow   Next element
VO + Left Arrow    Previous element
VO + Shift + Down  Enter group
VO + Shift + Up    Exit group

Reading:
VO + A             Read all from cursor
Ctrl               Stop speaking
VO + B             Read current paragraph

Interaction:
VO + Space         Activate element
VO + Shift + M     Open menu
Tab                Next focusable element
Shift + Tab        Previous focusable element

Rotor (VO + U):
Navigate by: Headings, Links, Forms, Landmarks
Left/Right Arrow   Change rotor category
Up/Down Arrow      Navigate within category
Enter              Go to item

Web Specific:
VO + Cmd + H       Next heading
VO + Cmd + J       Next form control
VO + Cmd + L       Next link
VO + Cmd + T       Next table
```

### Testing Checklist

```markdown
## VoiceOver Testing Checklist

### Page Load

- [ ] Page title announced
- [ ] Main landmark found
- [ ] Skip link works

### Navigation

- [ ] All headings discoverable via rotor
- [ ] Heading levels logical (H1 → H2 → H3)
- [ ] Landmarks properly labeled
- [ ] Skip links functional

### Links & Buttons

- [ ] Link purpose clear
- [ ] Button actions described
- [ ] New window/tab announced

### Forms

- [ ] All labels read with inputs
- [ ] Required fields announced
- [ ] Error messages read
- [ ] Instructions available
- [ ] Focus moves to errors

### Dynamic Content

- [ ] Alerts announced immediately
- [ ] Loading states communicated
- [ ] Content updates announced
- [ ] Modals trap focus correctly

### Tables

- [ ] Headers associated with cells
- [ ] Table navigation works
- [ ] Complex tables have captions
```

### Common Issues & Fixes

```html
<!-- Issue: Button not announcing purpose -->
<button><svg>...</svg></button>

<!-- Fix -->
<button aria-label="Close dialog"><svg aria-hidden="true">...</svg></button>

<!-- Issue: Dynamic content not announced -->
<div id="results">New results loaded</div>

<!-- Fix -->
<div id="results" role="status" aria-live="polite">New results loaded</div>

<!-- Issue: Form error not read -->
<input type="email" />
<span class="error">Invalid email</span>

<!-- Fix -->
<input type="email" aria-invalid="true" aria-describedby="email-error" />
<span id="email-error" role="alert">Invalid email</span>
```

## NVDA (Windows)

### Setup

```
Download: nvaccess.org
Start: Ctrl + Alt + N
Stop: Insert + Q
```

### Essential Commands

```
Navigation:
Insert = NVDA modifier

Down Arrow         Next line
Up Arrow           Previous line
Tab                Next focusable
Shift + Tab        Previous focusable

Reading:
NVDA + Down Arrow  Say all
Ctrl               Stop speech
NVDA + Up Arrow    Current line

Headings:
H                  Next heading
Shift + H          Previous heading
1-6                Heading level 1-6

Forms:
F                  Next form field
B                  Next button
E                  Next edit field
X                  Next checkbox
C                  Next combo box

Links:
K                  Next link
U                  Next unvisited link
V                  Next visited link

Landmarks:
D                  Next landmark
Shift + D          Previous landmark

Tables:
T                  Next table
Ctrl + Alt + Arrows Navigate cells

Elements List (NVDA + F7):
Shows all links, headings, form fields, landmarks
```

### Browse vs Focus Mode

```
NVDA automatically switches modes:
- Browse Mode: Arrow keys navigate content
- Focus Mode: Arrow keys control interactive elements

Manual switch: NVDA + Space

Watch for:
- "Browse mode" announcement when navigating
- "Focus mode" when entering form fields
- Application role forces forms mode
```

### Testing Script

```markdown
## NVDA Test Script

### Initial Load

1. Navigate to page
2. Let page finish loading
3. Press Insert + Down to read all
4. Note: Page title, main content identified?

### Landmark Navigation

1. Press D repeatedly
2. Check: All main areas reachable?
3. Check: Landmarks properly labeled?

### Heading Navigation

1. Press Insert + F7 → Headings
2. Check: Logical heading structure?
3. Press H to navigate headings
4. Check: All sections discoverable?

### Form Testing

1. Press F to find first form field
2. Check: Label read?
3. Fill in invalid data
4. Submit form
5. Check: Errors announced?
6. Check: Focus moved to error?

### Interactive Elements

1. Tab through all interactive elements
2. Check: Each announces role and state
3. Activate buttons with Enter/Space
4. Check: Result announced?

### Dynamic Content

1. Trigger content update
2. Check: Change announced?
3. Open modal
4. Check: Focus trapped?
5. Close modal
6. Check: Focus returns?
```

## JAWS (Windows)

### Essential Commands

```
Start: Desktop shortcut or Ctrl + Alt + J
Virtual Cursor: Auto-enabled in browsers

Navigation:
Arrow keys         Navigate content
Tab                Next focusable
Insert + Down      Read all
Ctrl               Stop speech

Quick Keys:
H                  Next heading
T                  Next table
F                  Next form field
B                  Next button
G                  Next graphic
L                  Next list
;                  Next landmark

Forms Mode:
Enter              Enter forms mode
Numpad +           Exit forms mode
F5                 List form fields

Lists:
Insert + F7        Link list
Insert + F6        Heading list
Insert + F5        Form field list

Tables:
Ctrl + Alt + Arrows Table navigation
```

## TalkBack (Android)

### Setup

```
Enable: Settings → Accessibility → TalkBack
Toggle: Hold both volume buttons 3 seconds
```

### Gestures

```
Explore: Drag finger across screen
Next: Swipe right
Previous: Swipe left
Activate: Double tap
Scroll: Two finger swipe

Reading Controls (swipe up then right):
- Headings
- Links
- Controls
- Characters
- Words
- Lines
- Paragraphs
```

## Common Test Scenarios

### 1. Modal Dialog

```html
<!-- Accessible modal structure -->
<div
  role="dialog"
  aria-modal="true"
  aria-labelledby="dialog-title"
  aria-describedby="dialog-desc"
>
  <h2 id="dialog-title">Confirm Delete</h2>
  <p id="dialog-desc">This action cannot be undone.</p>
  <button>Cancel</button>
  <button>Delete</button>
</div>
```

```javascript
// Focus management
function openModal(modal) {
  // Store last focused element
  lastFocus = document.activeElement;

  // Move focus to modal
  modal.querySelector("h2").focus();

  // Trap focus
  modal.addEventListener("keydown", trapFocus);
}

function closeModal(modal) {
  // Return focus
  lastFocus.focus();
}

function trapFocus(e) {
  if (e.key === "Tab") {
    const focusable = modal.querySelectorAll(
      'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
    );
    const first = focusable[0];
    const last = focusable[focusable.length - 1];

    if (e.shiftKey && document.activeElement === first) {
      last.focus();
      e.preventDefault();
    } else if (!e.shiftKey && document.activeElement === last) {
      first.focus();
      e.preventDefault();
    }
  }

  if (e.key === "Escape") {
    closeModal(modal);
  }
}
```

### 2. Live Regions

```html
<!-- Status messages (polite) -->
<div role="status" aria-live="polite" aria-atomic="true">
  <!-- Content updates will be announced after current speech -->
</div>

<!-- Alerts (assertive) -->
<div role="alert" aria-live="assertive">
  <!-- Content updates interrupt current speech -->
</div>

<!-- Progress updates -->
<div
  role="progressbar"
  aria-valuenow="75"
  aria-valuemin="0"
  aria-valuemax="100"
  aria-label="Upload progress"
></div>

<!-- Log (additions only) -->
<div role="log" aria-live="polite" aria-relevant="additions">
  <!-- New messages announced, removals not -->
</div>
```

### 3. Tab Interface

```html
<div role="tablist" aria-label="Product information">
  <button role="tab" id="tab-1" aria-selected="true" aria-controls="panel-1">
    Description
  </button>
  <button
    role="tab"
    id="tab-2"
    aria-selected="false"
    aria-controls="panel-2"
    tabindex="-1"
  >
    Reviews
  </button>
</div>

<div role="tabpanel" id="panel-1" aria-labelledby="tab-1">
  Product description content...
</div>

<div role="tabpanel" id="panel-2" aria-labelledby="tab-2" hidden>
  Reviews content...
</div>
```

```javascript
// Tab keyboard navigation
tablist.addEventListener("keydown", (e) => {
  const tabs = [...tablist.querySelectorAll('[role="tab"]')];
  const index = tabs.indexOf(document.activeElement);

  let newIndex;
  switch (e.key) {
    case "ArrowRight":
      newIndex = (index + 1) % tabs.length;
      break;
    case "ArrowLeft":
      newIndex = (index - 1 + tabs.length) % tabs.length;
      break;
    case "Home":
      newIndex = 0;
      break;
    case "End":
      newIndex = tabs.length - 1;
      break;
    default:
      return;
  }

  tabs[newIndex].focus();
  activateTab(tabs[newIndex]);
  e.preventDefault();
});
```

## Debugging Tips

```javascript
// Log what screen reader sees
function logAccessibleName(element) {
  const computed = window.getComputedStyle(element);
  console.log({
    role: element.getAttribute("role") || element.tagName,
    name:
      element.getAttribute("aria-label") ||
      element.getAttribute("aria-labelledby") ||
      element.textContent,
    state: {
      expanded: element.getAttribute("aria-expanded"),
      selected: element.getAttribute("aria-selected"),
      checked: element.getAttribute("aria-checked"),
      disabled: element.disabled,
    },
    visible: computed.display !== "none" && computed.visibility !== "hidden",
  });
}
```

## Best Practices

### Do's

- **Test with actual screen readers** - Not just simulators
- **Use semantic HTML first** - ARIA is supplemental
- **Test in browse and focus modes** - Different experiences
- **Verify focus management** - Especially for SPAs
- **Test keyboard only first** - Foundation for SR testing

### Don'ts

- **Don't assume one SR is enough** - Test multiple
- **Don't ignore mobile** - Growing user base
- **Don't test only happy path** - Test error states
- **Don't skip dynamic content** - Most common issues
- **Don't rely on visual testing** - Different experience


# wcag-audit-patterns

---
name: wcag-audit-patterns
description: Conduct WCAG 2.2 accessibility audits with automated testing, manual verification, and remediation guidance. Use when auditing websites for accessibility, fixing WCAG violations, or implementing accessible design patterns.
---

# WCAG Audit Patterns

Comprehensive guide to auditing web content against WCAG 2.2 guidelines with actionable remediation strategies.

## When to Use This Skill

- Conducting accessibility audits
- Fixing WCAG violations
- Implementing accessible components
- Preparing for accessibility lawsuits
- Meeting ADA/Section 508 requirements
- Achieving VPAT compliance

## Core Concepts

### 1. WCAG Conformance Levels

| Level   | Description            | Required For      |
| ------- | ---------------------- | ----------------- |
| **A**   | Minimum accessibility  | Legal baseline    |
| **AA**  | Standard conformance   | Most regulations  |
| **AAA** | Enhanced accessibility | Specialized needs |

### 2. POUR Principles

```
Perceivable:  Can users perceive the content?
Operable:     Can users operate the interface?
Understandable: Can users understand the content?
Robust:       Does it work with assistive tech?
```

### 3. Common Violations by Impact

```
Critical (Blockers):
├── Missing alt text for functional images
├── No keyboard access to interactive elements
├── Missing form labels
└── Auto-playing media without controls

Serious:
├── Insufficient color contrast
├── Missing skip links
├── Inaccessible custom widgets
└── Missing page titles

Moderate:
├── Missing language attribute
├── Unclear link text
├── Missing landmarks
└── Improper heading hierarchy
```

## Audit Checklist

### Perceivable (Principle 1)

````markdown
## 1.1 Text Alternatives

### 1.1.1 Non-text Content (Level A)

- [ ] All images have alt text
- [ ] Decorative images have alt=""
- [ ] Complex images have long descriptions
- [ ] Icons with meaning have accessible names
- [ ] CAPTCHAs have alternatives

Check:

```html
<!-- Good -->
<img src="chart.png" alt="Sales increased 25% from Q1 to Q2" />
<img src="decorative-line.png" alt="" />

<!-- Bad -->
<img src="chart.png" />
<img src="decorative-line.png" alt="decorative line" />
```
````

## 1.2 Time-based Media

### 1.2.1 Audio-only and Video-only (Level A)

- [ ] Audio has text transcript
- [ ] Video has audio description or transcript

### 1.2.2 Captions (Level A)

- [ ] All video has synchronized captions
- [ ] Captions are accurate and complete
- [ ] Speaker identification included

### 1.2.3 Audio Description (Level A)

- [ ] Video has audio description for visual content

## 1.3 Adaptable

### 1.3.1 Info and Relationships (Level A)

- [ ] Headings use proper tags (h1-h6)
- [ ] Lists use ul/ol/dl
- [ ] Tables have headers
- [ ] Form inputs have labels
- [ ] ARIA landmarks present

Check:

```html
<!-- Heading hierarchy -->
<h1>Page Title</h1>
<h2>Section</h2>
<h3>Subsection</h3>
<h2>Another Section</h2>

<!-- Table headers -->
<table>
  <thead>
    <tr>
      <th scope="col">Name</th>
      <th scope="col">Price</th>
    </tr>
  </thead>
</table>
```

### 1.3.2 Meaningful Sequence (Level A)

- [ ] Reading order is logical
- [ ] CSS positioning doesn't break order
- [ ] Focus order matches visual order

### 1.3.3 Sensory Characteristics (Level A)

- [ ] Instructions don't rely on shape/color alone
- [ ] "Click the red button" → "Click Submit (red button)"

## 1.4 Distinguishable

### 1.4.1 Use of Color (Level A)

- [ ] Color is not only means of conveying info
- [ ] Links distinguishable without color
- [ ] Error states not color-only

### 1.4.3 Contrast (Minimum) (Level AA)

- [ ] Text: 4.5:1 contrast ratio
- [ ] Large text (18pt+): 3:1 ratio
- [ ] UI components: 3:1 ratio

Tools: WebAIM Contrast Checker, axe DevTools

### 1.4.4 Resize Text (Level AA)

- [ ] Text resizes to 200% without loss
- [ ] No horizontal scrolling at 320px
- [ ] Content reflows properly

### 1.4.10 Reflow (Level AA)

- [ ] Content reflows at 400% zoom
- [ ] No two-dimensional scrolling
- [ ] All content accessible at 320px width

### 1.4.11 Non-text Contrast (Level AA)

- [ ] UI components have 3:1 contrast
- [ ] Focus indicators visible
- [ ] Graphical objects distinguishable

### 1.4.12 Text Spacing (Level AA)

- [ ] No content loss with increased spacing
- [ ] Line height 1.5x font size
- [ ] Paragraph spacing 2x font size
- [ ] Letter spacing 0.12x font size
- [ ] Word spacing 0.16x font size

````

### Operable (Principle 2)

```markdown
## 2.1 Keyboard Accessible

### 2.1.1 Keyboard (Level A)
- [ ] All functionality keyboard accessible
- [ ] No keyboard traps
- [ ] Tab order is logical
- [ ] Custom widgets are keyboard operable

Check:
```javascript
// Custom button must be keyboard accessible
<div role="button" tabindex="0"
     onkeydown="if(event.key === 'Enter' || event.key === ' ') activate()">
````

### 2.1.2 No Keyboard Trap (Level A)

- [ ] Focus can move away from all components
- [ ] Modal dialogs trap focus correctly
- [ ] Focus returns after modal closes

## 2.2 Enough Time

### 2.2.1 Timing Adjustable (Level A)

- [ ] Session timeouts can be extended
- [ ] User warned before timeout
- [ ] Option to disable auto-refresh

### 2.2.2 Pause, Stop, Hide (Level A)

- [ ] Moving content can be paused
- [ ] Auto-updating content can be paused
- [ ] Animations respect prefers-reduced-motion

```css
@media (prefers-reduced-motion: reduce) {
  * {
    animation: none !important;
    transition: none !important;
  }
}
```

## 2.3 Seizures and Physical Reactions

### 2.3.1 Three Flashes (Level A)

- [ ] No content flashes more than 3 times/second
- [ ] Flashing area is small (<25% viewport)

## 2.4 Navigable

### 2.4.1 Bypass Blocks (Level A)

- [ ] Skip to main content link present
- [ ] Landmark regions defined
- [ ] Proper heading structure

```html
<a href="#main" class="skip-link">Skip to main content</a>
<main id="main">...</main>
```

### 2.4.2 Page Titled (Level A)

- [ ] Unique, descriptive page titles
- [ ] Title reflects page content

### 2.4.3 Focus Order (Level A)

- [ ] Focus order matches visual order
- [ ] tabindex used correctly

### 2.4.4 Link Purpose (In Context) (Level A)

- [ ] Links make sense out of context
- [ ] No "click here" or "read more" alone

```html
<!-- Bad -->
<a href="report.pdf">Click here</a>

<!-- Good -->
<a href="report.pdf">Download Q4 Sales Report (PDF)</a>
```

### 2.4.6 Headings and Labels (Level AA)

- [ ] Headings describe content
- [ ] Labels describe purpose

### 2.4.7 Focus Visible (Level AA)

- [ ] Focus indicator visible on all elements
- [ ] Custom focus styles meet contrast

```css
:focus {
  outline: 3px solid #005fcc;
  outline-offset: 2px;
}
```

### 2.4.11 Focus Not Obscured (Level AA) - WCAG 2.2

- [ ] Focused element not fully hidden
- [ ] Sticky headers don't obscure focus

````

### Understandable (Principle 3)

```markdown
## 3.1 Readable

### 3.1.1 Language of Page (Level A)
- [ ] HTML lang attribute set
- [ ] Language correct for content

```html
<html lang="en">
````

### 3.1.2 Language of Parts (Level AA)

- [ ] Language changes marked

```html
<p>The French word <span lang="fr">bonjour</span> means hello.</p>
```

## 3.2 Predictable

### 3.2.1 On Focus (Level A)

- [ ] No context change on focus alone
- [ ] No unexpected popups on focus

### 3.2.2 On Input (Level A)

- [ ] No automatic form submission
- [ ] User warned before context change

### 3.2.3 Consistent Navigation (Level AA)

- [ ] Navigation consistent across pages
- [ ] Repeated components same order

### 3.2.4 Consistent Identification (Level AA)

- [ ] Same functionality = same label
- [ ] Icons used consistently

## 3.3 Input Assistance

### 3.3.1 Error Identification (Level A)

- [ ] Errors clearly identified
- [ ] Error message describes problem
- [ ] Error linked to field

```html
<input aria-describedby="email-error" aria-invalid="true" />
<span id="email-error" role="alert">Please enter valid email</span>
```

### 3.3.2 Labels or Instructions (Level A)

- [ ] All inputs have visible labels
- [ ] Required fields indicated
- [ ] Format hints provided

### 3.3.3 Error Suggestion (Level AA)

- [ ] Errors include correction suggestion
- [ ] Suggestions are specific

### 3.3.4 Error Prevention (Level AA)

- [ ] Legal/financial forms reversible
- [ ] Data checked before submission
- [ ] User can review before submit

````

### Robust (Principle 4)

```markdown
## 4.1 Compatible

### 4.1.1 Parsing (Level A) - Obsolete in WCAG 2.2
- [ ] Valid HTML (good practice)
- [ ] No duplicate IDs
- [ ] Complete start/end tags

### 4.1.2 Name, Role, Value (Level A)
- [ ] Custom widgets have accessible names
- [ ] ARIA roles correct
- [ ] State changes announced

```html
<!-- Accessible custom checkbox -->
<div role="checkbox"
     aria-checked="false"
     tabindex="0"
     aria-labelledby="label">
</div>
<span id="label">Accept terms</span>
````

### 4.1.3 Status Messages (Level AA)

- [ ] Status updates announced
- [ ] Live regions used correctly

```html
<div role="status" aria-live="polite">3 items added to cart</div>

<div role="alert" aria-live="assertive">Error: Form submission failed</div>
```

````

## Automated Testing

```javascript
// axe-core integration
const axe = require('axe-core');

async function runAccessibilityAudit(page) {
  await page.addScriptTag({ path: require.resolve('axe-core') });

  const results = await page.evaluate(async () => {
    return await axe.run(document, {
      runOnly: {
        type: 'tag',
        values: ['wcag2a', 'wcag2aa', 'wcag21aa', 'wcag22aa']
      }
    });
  });

  return {
    violations: results.violations,
    passes: results.passes,
    incomplete: results.incomplete
  };
}

// Playwright test example
test('should have no accessibility violations', async ({ page }) => {
  await page.goto('/');
  const results = await runAccessibilityAudit(page);

  expect(results.violations).toHaveLength(0);
});
````

```bash
# CLI tools
npx @axe-core/cli https://example.com
npx pa11y https://example.com
lighthouse https://example.com --only-categories=accessibility
```

## Remediation Patterns

### Fix: Missing Form Labels

```html
<!-- Before -->
<input type="email" placeholder="Email" />

<!-- After: Option 1 - Visible label -->
<label for="email">Email address</label>
<input id="email" type="email" />

<!-- After: Option 2 - aria-label -->
<input type="email" aria-label="Email address" />

<!-- After: Option 3 - aria-labelledby -->
<span id="email-label">Email</span>
<input type="email" aria-labelledby="email-label" />
```

### Fix: Insufficient Color Contrast

```css
/* Before: 2.5:1 contrast */
.text {
  color: #767676;
}

/* After: 4.5:1 contrast */
.text {
  color: #595959;
}

/* Or add background */
.text {
  color: #767676;
  background: #000;
}
```

### Fix: Keyboard Navigation

```javascript
// Make custom element keyboard accessible
class AccessibleDropdown extends HTMLElement {
  connectedCallback() {
    this.setAttribute("tabindex", "0");
    this.setAttribute("role", "combobox");
    this.setAttribute("aria-expanded", "false");

    this.addEventListener("keydown", (e) => {
      switch (e.key) {
        case "Enter":
        case " ":
          this.toggle();
          e.preventDefault();
          break;
        case "Escape":
          this.close();
          break;
        case "ArrowDown":
          this.focusNext();
          e.preventDefault();
          break;
        case "ArrowUp":
          this.focusPrevious();
          e.preventDefault();
          break;
      }
    });
  }
}
```

## Best Practices

### Do's

- **Start early** - Accessibility from design phase
- **Test with real users** - Disabled users provide best feedback
- **Automate what you can** - 30-50% issues detectable
- **Use semantic HTML** - Reduces ARIA needs
- **Document patterns** - Build accessible component library

### Don'ts

- **Don't rely only on automated testing** - Manual testing required
- **Don't use ARIA as first solution** - Native HTML first
- **Don't hide focus outlines** - Keyboard users need them
- **Don't disable zoom** - Users need to resize
- **Don't use color alone** - Multiple indicators needed
