const themeKey = 'ai-arena-theme';

function applyTheme(value) {
    const theme = value === 'light' || value === 'system' ? value : 'dark';
    document.documentElement.dataset.theme = theme;
    const control = document.getElementById('theme');
    if (control) control.value = theme;
}

function restoreTheme() {
    try {
        applyTheme(localStorage.getItem(themeKey));
    } catch {
        applyTheme(document.documentElement.dataset.theme);
    }
}

restoreTheme();
window.addEventListener('pageshow', restoreTheme);
window.addEventListener('storage', event => {
    if (event.key === themeKey || event.key === null) restoreTheme();
});
document.addEventListener('DOMContentLoaded', () => {
    const control = document.getElementById('theme');
    control.value = document.documentElement.dataset.theme;
    control.addEventListener('change', () => {
        applyTheme(control.value);
        try {
            localStorage.setItem(themeKey, control.value);
        } catch {
            // The selection still applies to this page when storage is unavailable.
        }
    });
});

function updateAgent(card, providerChanged) {
    const option = card.querySelector('[data-provider]').selectedOptions[0];
    const model = card.querySelector('[data-model-input]');
    if (providerChanged) {
        const suggestions = option.dataset.models.split(' ');
        card.querySelector('[data-model-list]').replaceChildren(...suggestions.map(id => new Option(id, id)));
        model.value = suggestions[0];
        card.querySelector('[data-field="profile"] input').value = '';
    }

    // A family entry includes its YYYY-MM-DD snapshots.
    const family = model.value.trim().replace(/-\d{4}-\d{2}-\d{2}$/, '');
    const supported = option.dataset.temperature.split(' ').filter(Boolean);
    const local = option.value === 'Ollama';
    const fields = {
        temperature: supported.includes('*') || supported.includes(family),
        endpoint: local,
        profile: !local,
    };
    for (const [name, enabled] of Object.entries(fields)) {
        const field = card.querySelector(`[data-field="${name}"]`);
        field.hidden = !enabled;
        field.querySelector('input').disabled = !enabled;
    }
}

document.addEventListener('input', event => {
    if (event.target.matches('[data-model-input]')) {
        updateAgent(event.target.closest('[data-agent]'), false);
    }
});
document.addEventListener('change', event => {
    if (event.target.matches('[data-provider]')) {
        updateAgent(event.target.closest('[data-agent]'), true);
    }
    const form = event.target.closest('form[action="/games/start"]');
    if (form && form.checkValidity()) {
        htmx.ajax('POST', '/games/defaults', {source: form, swap: 'none'});
    }
});
document.addEventListener('htmx:beforeSwap', event => {
    if (event.detail.xhr.status === 400 && ['start-form', 'matrix-form', 'matrix-feedback'].includes(event.detail.target.id)) {
        event.detail.shouldSwap = true;
        event.detail.isError = false;
    }
});

// Keep disclosures attached to the model when fresh scores reorder the table.
const expandedModels = new Set();

function showOpponents(button, expanded) {
    button.setAttribute('aria-expanded', String(expanded));
    document.getElementById(button.getAttribute('aria-controls')).hidden = !expanded;
}

document.addEventListener('click', event => {
    const button = event.target.closest('.model-toggle');
    if (!button) return;
    const expand = button.getAttribute('aria-expanded') === 'false';
    if (expand) expandedModels.add(button.id);
    else expandedModels.delete(button.id);
    showOpponents(button, expand);
});
document.addEventListener('htmx:afterSwap', event => {
    for (const button of event.detail.target.querySelectorAll('.model-toggle')) {
        if (expandedModels.has(button.id)) showOpponents(button, true);
    }
});

function updateMatrix(form) {
    const cards = Array.from(form.querySelectorAll('[data-agent]'));
    const repetitions = form.elements.repetitions;
    const pairs = cards.length * (cards.length - 1) / 2;
    const total = pairs * repetitions.valueAsNumber;
    const output = form.querySelector('[data-match-count]');
    const limit = Number(output.dataset.limit);
    const tooMany = total > limit;
    repetitions.setCustomValidity(tooMany ? `Choose at most ${limit} total matches.` : '');
    output.textContent = Number.isInteger(total) && total > 0
        ? `${pairs} ${pairs === 1 ? 'pair' : 'pairs'} · ${total} ${total === 1 ? 'match' : 'matches'} total${tooMany ? ` (limit ${limit})` : ''}`
        : 'Enter the number of matches per pair.';
    cards.forEach((card, index) => {
        card.querySelectorAll('[data-seat]').forEach(label => { label.textContent = index + 1; });
        card.querySelector('[data-remove-agent]').disabled = cards.length <= 2;
    });
}

document.addEventListener('click', event => {
    const button = event.target.closest('[data-add-agent], [data-remove-agent]');
    if (!button) return;
    const form = button.form;
    if (button.hasAttribute('data-add-agent')) {
        const slot = Number(form.dataset.nextSlot || 3);
        form.dataset.nextSlot = slot + 1;
        const card = form.querySelector('template').content.firstElementChild.cloneNode(true);
        for (const input of card.querySelectorAll('[name], [list], [id]')) {
            for (const attr of ['name', 'list', 'id']) {
                if (input.hasAttribute(attr)) {
                    input.setAttribute(attr, input.getAttribute(attr).replace('newslot', 'slot' + slot));
                }
            }
        }
        form.querySelector('#agent-list').append(card);
        card.querySelector('[data-provider]').focus();
    } else {
        button.closest('[data-agent]').remove();
        form.querySelector('[data-add-agent]').focus();
    }
    updateMatrix(form);
});
document.addEventListener('input', event => {
    if (event.target.name === 'repetitions' && event.target.form?.querySelector('[data-match-count]')) {
        updateMatrix(event.target.form);
    }
});
