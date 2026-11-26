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
    const menu = document.querySelector('.section-menu');
    document.addEventListener('click', event => {
        if (menu.open && !menu.contains(event.target)) menu.open = false;
    });
    menu.addEventListener('focusout', event => {
        if (event.relatedTarget && !menu.contains(event.relatedTarget)) menu.open = false;
    });
    menu.addEventListener('keydown', event => {
        if (event.key === 'Escape' && menu.open) {
            menu.open = false;
            menu.querySelector('summary').focus();
        }
    });
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

function updateBenchForm(form) {
    const mock = form.elements.dry_run.checked;
    const models = form.elements.models.value.split(/[\s,]+/).filter(Boolean);
    const ollama = models.length > 0 && models.every(model => /^ollama:/i.test(model));
    form.elements.models.disabled = mock;
    form.elements.profiles.disabled = mock;
    form.querySelector('.bench-models').disabled = mock;
    form.querySelector('[data-add-model]').setCustomValidity(
        !mock && models.length === 0 ? 'Add at least one model or select Offline mock.' : ''
    );
    form.elements.server.disabled = mock || !ollama;
    form.querySelector('[data-ollama-endpoint]').hidden = form.elements.server.disabled;
}

function initBenchModels(form) {
    const picker = form.querySelector('[data-model-picker]');
    const select = picker.querySelector('[data-add-model]');
    const presets = select.querySelector('optgroup');
    const chips = picker.querySelector('.model-chips');
    const custom = picker.querySelector('.custom-model');
    const provider = custom.querySelector('[data-custom-provider]');
    const modelId = custom.querySelector('[data-custom-id]');
    const feedback = custom.querySelector('[data-model-feedback]');
    const input = form.elements.models;
    const selected = () => new Set(input.value.split(/[\s,]+/).filter(Boolean));

    function render() {
        const models = selected();
        chips.replaceChildren();
        for (const model of models) {
            const item = document.createElement('li');
            const remove = document.createElement('button');
            remove.type = 'button';
            remove.textContent = '×';
            remove.setAttribute('aria-label', 'Remove ' + model);
            remove.addEventListener('click', () => {
                const remaining = selected();
                remaining.delete(model);
                input.value = [...remaining].join('\n');
                render();
                select.focus();
            });
            item.append(model, remove);
            chips.append(item);
        }
        for (const option of presets.children) option.disabled = models.has(option.value);
        updateBenchForm(form);
    }

    function add(model) {
        input.value = [...selected().add(model)].join('\n');
        select.value = '';
        custom.hidden = true;
        modelId.value = '';
        render();
        select.focus();
    }

    select.addEventListener('change', () => {
        custom.hidden = select.value !== 'custom';
        feedback.textContent = '';
        if (!custom.hidden) provider.focus();
        else if (select.value) add(select.value);
    });
    async function addCustom(save) {
        const id = modelId.value.trim();
        if (!id || /[\s,]/.test(id)) {
            feedback.textContent = 'Enter a model ID without spaces or commas.';
            modelId.focus();
            return;
        }
        const model = provider.value + ':' + id;
        if (save) {
            feedback.textContent = 'Saving…';
            custom.disabled = true;
            const launch = form.querySelector('button[type=submit]');
            launch.disabled = true;
            try {
                const response = await fetch('/toolbench/models', {
                    method: 'POST',
                    body: new URLSearchParams({provider: provider.value, model: id}),
                });
                const data = await response.json();
                if (!response.ok) throw new Error(data.error || 'Could not save model.');
                presets.replaceChildren(...data.map(name => new Option(name, name)));
            } catch (error) {
                feedback.textContent = error.message;
                return;
            } finally {
                custom.disabled = false;
                launch.disabled = false;
            }
        }
        add(model);
    }
    custom.querySelector('[data-add-custom]').addEventListener('click', () => addCustom(false));
    custom.querySelector('[data-save-model]').addEventListener('click', () => addCustom(true));
    modelId.addEventListener('keydown', event => {
        if (event.key === 'Enter') {
            event.preventDefault();
            addCustom(false);
        }
    });
    input.addEventListener('input', render);
    picker.hidden = false;
    form.querySelector('[data-model-editor]').open = false;
    render();
}

function formatBenchTimes(root) {
    for (const element of root.querySelectorAll('time[data-bench-time]')) {
        const date = new Date(Number(element.dataset.benchTime) * 1000);
        element.dateTime = date.toISOString();
        element.textContent = date.toLocaleString();
    }
}

document.addEventListener('DOMContentLoaded', () => {
    const form = document.querySelector('.bench-form');
    if (form) {
        initBenchModels(form);
        form.addEventListener('input', () => updateBenchForm(form));
    }
    formatBenchTimes(document);
});
document.addEventListener('htmx:afterSwap', event => formatBenchTimes(event.detail.target));
