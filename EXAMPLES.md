# tellme — Features by Example

> A walk-through of every feature using one realistic codebase. The goal: make each feature *click*.

---

## The Setting

It's three weeks ago. You're building a checkout service with an AI agent. You ship it, it works, you move on.

Today you open `checkout.py` to add a discount feature — and you have no idea what past-you (and the agent) were thinking. Let's use **tellme** to find out.

Here's the file you're staring at:

```python
# checkout.py
def calculate_total(cart, user):
    subtotal = sum(item.price * item.qty for item in cart.items)
    tax = subtotal * 0.08
    shipping = 0 if subtotal > 50 else 7.99    # line 4
    total = subtotal + tax + shipping
    if user.is_premium:
        total = total * 0.9                     # line 7
    return round(total, 2)
```

---

## 1. Prompt Blame — "tell me why this line exists"

**The moment it clicks:** You see `shipping = 0 if subtotal > 50 else 7.99` on line 4. Why `$50`? Why `$7.99`? You don't remember deciding either number.

```
$ tellme why checkout.py #line4
```

```
checkout.py:4  shipping = 0 if subtotal > 50 else 7.99

┌─ Prompt history for this line ──────────────────────────────┐
│                                                              │
│  ● 3 weeks ago — agent session "checkout-v1"                 │
│    YOU: "add free shipping over $50, otherwise flat $7.99"   │
│    → line created                                            │
│                                                              │
│  ● 2 weeks ago — agent session "fix-shipping-edge"           │
│    YOU: "the free shipping threshold should be exclusive,    │
│         exactly $50 should still be charged shipping"        │
│    → changed `>=` to `>`                                     │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

**Now it clicks:** The `$50`/`$7.99` weren't arbitrary — they came from a product requirement. And there was a deliberate fix about the boundary being *exclusive*. You almost re-introduced that bug. tellme just saved you.

---

## 2. Data Flow — Variables

**The moment it clicks:** You want to change how `total` is calculated, but it's touched in several places and you're scared of breaking the premium discount. Where does `total` actually live and die?

```
$ tellme flow checkout.py --var total
```

```
Variable: total   (checkout.py, function calculate_total)

  ▸ INITIALIZED   line 5   total = subtotal + tax + shipping
        │
  ▸ MODIFIED      line 7   total = total * 0.9   (premium discount)
        │
  ▸ USED          line 8   return round(total, 2)
        │
  ▸ LIFECYCLE END line 8   returned, goes out of scope
```

**Now it clicks:** `total` is born on line 5, gets a 10% premium haircut on line 7, and leaves the building on line 8. Three touchpoints, fully mapped. You know exactly what your change will affect.

---

## 2.5 Cross-Layer Data Flow — "watch the data travel from DB to API"

**The moment it clicks:** Section 2 showed a variable's life *inside one function*. But the real question is bigger: your controller `items()` returns all items — where does that data actually *come from*, and what touches it on the way out to the client?

Here's the code spread across layers:

```python
# repository/items.py
def fetch_all():
    return db.query("SELECT id, name, price, stock FROM items")   # raw rows

# service/items.py
def list_items():
    rows = fetch_all()
    return [Item(**r) for r in rows]            # rows → Item models

# controller/items.py
def items():
    models = list_items()
    return jsonify([i.to_dto() for i in models])  # models → DTO → JSON
```

```
$ tellme journey controller/items.py --endpoint items
```

```
Data journey for endpoint: items()

  ┌────────────────┐     ┌──────────────────┐     ┌─────────────────┐
  │   DB: items    │     │   repository     │     │     service     │
  │  table (rows)  │ ──▶ │  fetch_all()     │ ──▶ │  list_items()   │
  │ id,name,price, │     │  returns raw     │     │  rows → Item    │
  │ stock          │     │  query rows      │     │  models         │
  └────────────────┘     └──────────────────┘     └────────┬────────┘
                                                            │
       ┌────────────────────┐     ┌──────────────────┐     │
       │   API response     │     │   controller     │     │
       │  JSON array of     │ ◀── │  items()         │ ◀───┘
       │  item DTOs         │     │  Item → DTO →    │
       │                    │     │  jsonify         │
       └────────────────────┘     └──────────────────┘

  Transformations along the way:
    raw row  →  Item model   (service/items.py:list_items)
    Item     →  DTO          (controller/items.py:items via to_dto)
    DTO      →  JSON         (controller/items.py:items via jsonify)
```

**Now it clicks:** You can *see* the data physically move — DB table → repository → service → controller → API response — and exactly where each shape change happens. When the API returns the wrong field or a `null`, you know which box to inspect instead of grepping four files blind.

---

## 3. Data Flow — Functions

**The moment it clicks:** `calculate_total` is called all over the app and you're not sure what depends on it or what *it* depends on.

```
$ tellme flow checkout.py --function calculate_total
```

```
Function: calculate_total

  CALLED BY                        CALLS
  ─────────────                    ─────────────
  • api/routes.py:checkout()       • cart.items (property)
  • tests/test_checkout.py         • round() (builtin)

  SIGNATURE
  ─────────────
  in:  cart: Cart, user: User
  out: float
```

**Now it clicks:** Two callers (a route and a test), it leans on `cart.items` and `round()`, and it takes a `Cart` + `User` and returns a `float`. You now know the blast radius before you touch a thing.

---

## 4. The Flow Graph (visualizing 2 & 3)

Both flow commands can render a graph instead of a list:

```
$ tellme flow checkout.py --function calculate_total --graph
```

```
        api/routes.py            tests/test_checkout.py
              │                          │
              └────────────┬─────────────┘
                           ▼
                  ┌──────────────────┐
                  │ calculate_total  │
                  │ (cart, user)→flt │
                  └────────┬─────────┘
                           │ uses
                  ┌────────┴─────────┐
                  ▼                  ▼
              cart.items          round()
```

**Now it clicks:** You *see* the shape of the dependency instead of reading it. The graph is also where decisions get attached (next feature).

---

## 5. Decision Editor — "let me write down WHY"

**The moment it clicks:** Looking at the graph, you remember *why* the premium discount is applied last instead of before tax — and you don't want to forget again. You attach a decision to that node.

```
$ tellme decision add checkout.py --var total --line 7
```

```
Opening decision editor for  checkout.py:7  (total = total * 0.9)

  ┌────────────────────────────────────────────────────────┐
  │ WHY:                                                     │
  │ Premium discount applies AFTER tax on purpose — finance  │
  │ requires tax to be calculated on the full pre-discount   │
  │ amount for compliance. Do NOT move this above the tax    │
  │ line.                                                    │
  │                                                          │
  │ Decided by: you · linked to prompt "checkout-v1"         │
  └────────────────────────────────────────────────────────┘

  ✓ Decision saved and attached to total @ line 7
```

Next time anyone (including future-you) runs `tellme why checkout.py #line7`, this decision shows up right next to the prompt.

**Now it clicks:** The tool extracted the *structure*; you added the *intent*. Together they're real documentation — the kind that usually evaporates the second a chat window closes.

---

## 6. History — "the full story over time"

**The moment it clicks:** A teammate asks "has the premium discount logic always been 10%? When did this change?" Add `--history` to any flow query.

```
$ tellme flow checkout.py --var total --history
```

```
History: total   (checkout.py)

  ● CREATED        3 weeks ago     session "checkout-v1"
    PROMPT: "calculate the cart total with tax and shipping"
    DECISION: (none yet)

  ● MODIFIED       2 weeks ago     session "add-premium-tier"
    PROMPT: "give premium users a 10% discount on the total"
    DECISION: "Discount applies after tax for compliance" ← added today

  ● LAST MODIFIED  2 weeks ago
    No changes since.
```

**Now it clicks:** One command gives you the whole narrative — *when* it was created, *when* it last changed, the *prompts* that drove each change, and the *decisions* attached along the way. Code change + decision + prompt, stitched into a single timeline.

---

## Putting It All Together

| You ask… | Command | You get… |
|----------|---------|----------|
| Why does this line exist? | `tellme why checkout.py #line4` | The prompts that created/changed it |
| Where does this variable live? | `tellme flow checkout.py --var total` | Init → modify → use → end |
| What's connected to this function? | `tellme flow checkout.py --function calculate_total` | Callers, callees, types |
| Show me the shape | `… --graph` | A visual flow graph |
| Let me record the reasoning | `tellme decision add …` | An attached, persistent "why" |
| Tell me the whole story | `… --history` | Timeline of code + prompts + decisions |

The throughline: **every line is connected to the prompt that made it and the decision that justifies it — and you can ask about any of it, anytime.**
