 import _ from 'lodash';
 import Chart from 'chart.js'
 import seed from 'seed-random'

function dateMillisFromKey(key) {
    return parseInt(key.date.$date.$numberLong);
}

function populateDropdown(dropdown, keys) {
    _.forEach(keys, key => {
        let dateMillis = dateMillisFromKey(key);
        let date = new Date(dateMillis);
        const option = document.createElement('option');
        option.innerHTML = key.branch + "@" + key.commit + " | " + date.toISOString();
        option.value = dateMillis;
        dropdown.appendChild(option);
    });
}

function canvas() {
    const canvas = document.createElement('canvas');
    canvas.height = 100;
    return canvas;
}

seed('dimforge', {global: true});

function removeAllChildNodes(parent) {
    while (parent.firstChild) {
        parent.removeChild(parent.firstChild);
    }
}

function getRandomColor() {
  var letters = '0123456789ABCDEF';
  var color = '#';
  for (var i = 0; i < 6; i++) {
    color += letters[Math.floor(Math.random() * 16)];
  }
  return color;
}

let colors = new Map();
let dropdown1 = document.getElementById('dropdown1');
let dropdown2 = document.getElementById('dropdown2');
let graphsContainer = document.getElementById('graphs-container');
let checkboxOtherEngines = document.getElementById('checkbox-other-engines');

function reloadBenchmarks() {
    let date1 = dropdown1.value;
    let date2 = dropdown2.value;
    let showOtherEngines = checkboxOtherEngines.checked;
    let url = 'https://benchmarks.dimforge.com/graph/csv?project=rapier3d&date1=' + date1 + '&date2=' + date2 + '&otherEngines=' + showOtherEngines;
    fetch(url)
        .then(response => response.json())
        .then(data => {
            let key1 = data.entries1[0].key;
            let key2 = data.entries2[0].key;
            let pltf1 = data.entries1[0].platform;
            let pltf2 = data.entries2[0].platform;
            let titlePart1 = '{' + key1.branch + '@' + key1.commit + '}';
            let titlePart2 = '{' + key2.branch + '@' + key2.commit + '}';
            let titleTail = pltf1.compiler == pltf2.compiler ?
                pltf1.compiler : "{" + pltf1.compiler + " vs. " + pltf2.compiler + "}";

            let filteredEntries2 = _.filter(data.entries2, e => e.context.backend == 'rapier');
            _.forEach(filteredEntries2, e => e.context.backend = 'rapier ' + titlePart2);
            _.forEach(data.entries1, e => {
                if (e.context.backend == 'rapier') {
                    e.context.backend = 'rapier ' + titlePart1;
                }
            });
            let allEntries = _.concat(data.entries1, filteredEntries2);
            let groupedEntries = _.groupBy(allEntries, e => e.context.name);
            removeAllChildNodes(graphsContainer);

            _.forEach(groupedEntries, (entries, name) => {
                let title = name + " − " + titlePart1 + " vs. " + titlePart2 + " - " + titleTail;
                let labels = _.range(0, entries[0].timings.length);
                let datasets = entries.map(entry => {
                    if (!colors.get(entry.context.backend)) {
                        colors.set(entry.context.backend, getRandomColor());
                    }

                    return {
                        label: entry.context.backend,
                        data: entry.timings,
                        fill: false,
                        borderColor: [
                            colors.get(entry.context.backend)
                        ],
                        borderWidth: 2,
                        pointRadius: 0
                    };
                })

                let chartCanvas = canvas();
                graphsContainer.appendChild(chartCanvas);
                var ctx = chartCanvas.getContext('2d');
                new Chart(ctx, {
                    type: 'line',
                    data: {
                        labels: labels,
                        datasets: datasets
                    },
                    options: {
                        title: {
                            display: true,
                            text: title
                        },
                        scales: {
                            yAxes: [{
                                ticks: {
                                    beginAtZero: true,
                                    callback: function(value, index, values) {
                                        return value + 'ms';
                                    }
                                }
                            }]
                        }
                    }
                });
            });
        });
}

dropdown1.onchange = reloadBenchmarks;
dropdown2.onchange = reloadBenchmarks;
checkboxOtherEngines.onchange = reloadBenchmarks;

fetch('https://benchmarks.dimforge.com/list?project=rapier3d&field=key')
    .then(response => response.json())
    .then(data => {
        data.sort(function(a, b) {
            return dateMillisFromKey(a) < dateMillisFromKey(b);
        });
        console.log(data);
        populateDropdown(dropdown1, data);
        populateDropdown(dropdown2, data);
        // See if the URL has the ref to select.
        let url = new URL(window.location);
        let date1 = url.searchParams.get("date1");
        let date2 = url.searchParams.get("date2");

        if (!!date1 && !!data.includes(date1))
            dropdown1.value = date1;
        else if (dropdown1.length > 1)
            dropdown1.value = dateMillisFromKey(data[1]);
        if (!!date2 && !!data.includes(date2))
            dropdown2.value = date2;
        else if (data.length > 0) {
            dropdown2.value = dateMillisFromKey(data[0]);
        }

        reloadBenchmarks();
    });
