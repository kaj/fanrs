/**
 * Table sorter based on https://github.com/tofsjonas/sortable 1.0
 * Copyleft 2017 Jonas Earendel
 * For copyright information, please refer to <http://unlicense.org>
 */

document.addEventListener( 'click', function ( e ) {

    var down_class = 'dir-d';
    var up_class = 'dir-u';
    var element = e.target;

    if ( element.nodeName == 'TH' ) {

        var table = element.offsetParent;

        // make sure it is a sortable table
        if ( table.classList.contains('sortable') ) {

            var column_index;
            var tr = element.parentNode;
            var nodes = tr.cells;

            // reset thead cells and get column index
            for ( var i = 0; i < nodes.length; i++ ) {
                if ( nodes[ i ] === element ) {
                    column_index = i;
                } else {
                    nodes[i].classList.remove(down_class);
                    nodes[i].classList.remove(up_class);
                }
            }

            var dir = down_class;

            // check if we're sorting up or down, and update the css accordingly
            if ( element.classList.contains( down_class ) ) {
                dir = up_class;
            }

            element.classList.remove(down_class);
            element.classList.remove(up_class);
            element.classList.add(dir);

            // extract all table rows, so the sorting can start.
            var org_tbody = table.tBodies[ 0 ];

            // slightly faster if cloned, noticable for huge tables.
            var rows = [].slice.call( org_tbody.cloneNode( true ).rows, 0 );

            var reverse = ( dir == up_class );

            function getValue(x) {
                return x.getAttribute('data-sort') || x.innerText;
            }

            // sort them using custom built in array sort.
            rows.sort( function ( a, b ) {
                a = getValue(a.cells[column_index]);
                b = getValue(b.cells[column_index]);
                if ( reverse ) {
                    var c = a;
                    a = b;
                    b = c;
                }
                return isNaN( a - b ) ? a.localeCompare( b ) : a - b;
            } );

            // Make a clone without contents
            var clone_tbody = org_tbody.cloneNode();

            // Build a sorted table body and replace the old one.
            // (IE 11 dont support for row of rows)
            for ( i = 0; i < rows.length; i++ ) {
                clone_tbody.appendChild( rows[ i ] );
            }

            // And finally insert the end result
            table.replaceChild( clone_tbody, org_tbody );
        }

    }

} );
