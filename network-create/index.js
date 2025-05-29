var n = 3;
var deg = 2;

connectionsNum = [];
connections = [];
for(i = 0; i<n; i++) {
    connectionsNum.push(0);
    connections.push([]);
}

for(i = 0; i<n; i++) {
    while(connectionsNum[i] < deg){
        var next = Math.floor(Math.random() * n);
        if(connections[next].indexOf(i) < 0 && next != i) {
            connectionsNum[next]++;
            connectionsNum[i]++;
            connections[i].push(next);
            connections[next].push(i);
        }
    }
}

for(i = 0; i<n; i++) {
    process.stdout.write(`${i} `);
    console.log(...connections[i]);
}