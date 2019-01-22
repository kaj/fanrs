(function(form) {
    let list = document.createElement('div');
    list.className = 'list';
    let tags = form.querySelector('div.refs');
    if (!tags) {
	tags = document.createElement('div');
	tags.className='refs';
	form.appendChild(tags);
    }
    let kindname = { 't': 'serie', 'p': 'upphovsperson',
                     'k': 'i serien', 'f': 'Fantomenätten' }

    form.insertBefore(list, tags);
    let input = form.querySelector('input');
    input.autocomplete = "off";
    function addTag(title, kind, slug) {
	let s = document.createElement('label');
	s.innerHTML = title + ' <input type="checkbox" checked name="' + kind +
	    '" value="' + slug + '">';
	s.tabIndex = 4;
	s.className = kind;
	tags.appendChild(s);
	list.innerHTML = '';
	input.value = '';
	input.focus();
	return false;
    }
    input.addEventListener('keyup', e => {
	let v = e.target.value;
	if (v.length > 1) {
	    let r = new XMLHttpRequest();
	    r.onload = function() {
		let t = JSON.parse(this.responseText);
		list.innerHTML = '';
		t.map(x => {
		    let a = document.createElement('a');
		    a.innerHTML = x.t + ' <small>(' + kindname[x.k] + ')</small>';
		    a.className='hit ' + x.k;
		    a.href = x.s;
		    a.tabIndex = 2;
		    a.onclick = function() { return addTag(x.t, x.k, x.s) }
		    list.appendChild(a)
		})
	    };
	    r.open('GET', document.location.origin + '/ac?q=' + encodeURIComponent(v));
	    r.send(null);
	} else {
	    list.innerHTML = '';
	}
    })
    form.addEventListener('keypress', e => {
	let t = e.target;
	switch(e.code) {
	case 'ArrowUp':
	    (t.parentNode == list && t.previousSibling || list.querySelector('a:last-child')).focus();
	    break;
	case 'ArrowDown':
	    (t.parentNode == list && t.nextSibling || list.querySelector('a:first-child')).focus();
	    break;
	case 'Escape':
	    input.focus();
	    break;
	default:
	    return true;
	};
	e.preventDefault();
	e.stopPropagation();
	return false;
    });
    form.querySelector('.help .js').innerHTML = 'Du kan begränsa din sökning till de taggar som föreslås.';
})(document.querySelector('form#search'));
